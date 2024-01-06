use crate::{auth::SqliteUser, error, state::AppState};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Json},
    routing::{delete, get, post},
    Form, Router,
};
use libseed::collection::{Collection, Filter};
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};
use std::sync::Arc;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_collections))
        .route("/new", post(add_collection))
        .route("/:id/sample/:sampleid", delete(remove_sample))
        .route("/:id/add", post(add_sample))
        .route(
            "/:id",
            get(show_collection)
                .put(modify_collection)
                .delete(delete_collection),
        )
}

async fn root() -> Html<String> {
    Html("Collections".to_string())
}

async fn list_collections(
    user: SqliteUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let collections =
        Collection::fetch_all(Some(Arc::new(Filter::User(user.id))), &state.dbpool).await?;
    Ok(Json(collections).into_response())
}

async fn show_collection(
    _user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Collection>, error::Error> {
    let mut collection = Collection::fetch(id, &state.dbpool).await?;
    collection.fetch_samples(&state.dbpool).await?;
    Ok(Json(collection))
}

#[derive(Deserialize)]
struct ModifyProps {
    name: Option<String>,
    description: Option<String>,
}

async fn modify_collection(
    _user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<ModifyProps>,
) -> Result<(), error::Error> {
    if params.name.is_none() && params.description.is_none() {
        return Err(anyhow!("No params to modify").into());
    }
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE sc_collections SET ");
    let mut sep = builder.separated(", ");
    if let Some(name) = params.name {
        sep.push(" name=");
        sep.push_bind_unseparated(name);
    }
    if let Some(desc) = params.description {
        sep.push(" description=");
        sep.push_bind_unseparated(desc);
    }
    builder.push(" WHERE id=");
    builder.push_bind(id);
    builder.build().execute(&state.dbpool).await?;
    Ok(())
}

async fn delete_collection(
    _user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(), error::Error> {
    sqlx::query("DELETE FROM sc_collections WHERE id=?")
        .bind(id)
        .execute(&state.dbpool)
        .await?;
    Ok(())
}

#[derive(Deserialize)]
struct AddProps {
    name: String,
    description: Option<String>,
}

async fn add_collection(
    _user: SqliteUser,
    Query(params): Query<AddProps>,
    State(state): State<AppState>,
) -> Result<Json<i64>, error::Error> {
    let id = sqlx::query(
        "INSERT INTO sc_collections (name, description) VALUES (?,
    ?)",
    )
    .bind(params.name)
    .bind(params.description)
    .execute(&state.dbpool)
    .await?
    .last_insert_rowid();
    Ok(Json(id))
}

#[derive(Deserialize)]
struct RemoveSampleParams {
    id: i64,
    sampleid: i64,
}

async fn remove_sample(
    _user: SqliteUser,
    State(state): State<AppState>,
    Path(RemoveSampleParams { id, sampleid }): Path<RemoveSampleParams>,
) -> Result<(), error::Error> {
    sqlx::query("DELETE FROM sc_collection_samples WHERE collectionid=? AND sampleid=?")
        .bind(id)
        .bind(sampleid)
        .execute(&state.dbpool)
        .await?;
    Ok(())
}

#[derive(Deserialize)]
struct AddSampleProps {
    sample: i64,
}

async fn add_sample(
    _user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<AddSampleProps>,
) -> Result<Json<i64>, error::Error> {
    let id =
        sqlx::query("INSERT INTO sc_collection_samples (collectionid, sampleid) VALUES (?, ?)")
            .bind(id)
            .bind(params.sample)
            .execute(&state.dbpool)
            .await?
            .last_insert_rowid();
    Ok(Json(id))
}

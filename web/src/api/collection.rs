use crate::{error, state::AppState};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, Query, State},
    response::{Html, Json},
    routing::{delete, get, post},
    Form, Router,
};
use libseed::{
    collection::Collection,
    filter::Cmp,
    sample::{self, Filter},
};
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};

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
    State(state): State<AppState>,
) -> Result<Json<Vec<Collection>>, error::Error> {
    let collections: Vec<Collection> =
        sqlx::query_as("SELECT id, name, description FROM seedcollections")
            .fetch_all(&state.dbpool)
            .await?;
    Ok(Json(collections))
}

async fn show_collection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Collection>, error::Error> {
    let mut builder: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT id, name, description FROM seedcollections WHERE id=");
    builder.push_bind(id);
    let mut collection: Collection = builder.build_query_as().fetch_one(&state.dbpool).await?;
    let mut builder = sample::build_query(Some(Box::new(Filter::Collection(Cmp::Equal, id))));
    collection.samples = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(Json(collection))
}

#[derive(Deserialize)]
struct ModifyProps {
    name: Option<String>,
    description: Option<String>,
}

async fn modify_collection(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<ModifyProps>,
) -> Result<(), error::Error> {
    if params.name.is_none() && params.description.is_none() {
        return Err(anyhow!("No params to modify").into());
    }
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE seedcollections SET ");
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
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(), error::Error> {
    sqlx::query("DELETE FROM seedcollections WHERE id=?")
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
    Query(params): Query<AddProps>,
    State(state): State<AppState>,
) -> Result<Json<i64>, error::Error> {
    let id = sqlx::query(
        "INSERT INTO seedcollections (name, description) VALUES (?,
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
    State(state): State<AppState>,
    Path(RemoveSampleParams { id, sampleid }): Path<RemoveSampleParams>,
) -> Result<(), error::Error> {
    sqlx::query("DELETE FROM seedcollectionsamples WHERE collectionid=? AND sampleid=?")
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
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<AddSampleProps>,
) -> Result<Json<i64>, error::Error> {
    let id =
        sqlx::query("INSERT INTO seedcollectionsamples (collectionid, sampleid) VALUES (?, ?)")
            .bind(id)
            .bind(params.sample)
            .execute(&state.dbpool)
            .await?
            .last_insert_rowid();
    Ok(Json(id))
}

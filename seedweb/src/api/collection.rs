use crate::{error, state::SharedState};
use anyhow::anyhow;
use anyhow::Result;
use axum::routing::post;
use axum::Form;
use axum::{
    extract::{Path, Query, State},
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::{collection::Collection, sample};
use serde::Deserialize;
use sqlx::{QueryBuilder, Sqlite};
use std::sync::Arc;

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_collections))
        .route("/new", post(add_collection))
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
    State(state): State<Arc<SharedState>>,
) -> Result<Json<Vec<Collection>>, error::Error> {
    let collections: Vec<Collection> =
        sqlx::query_as("SELECT L.id, L.name, L.description FROM seedcollections L")
            .fetch_all(&state.dbpool)
            .await?;
    Ok(Json(collections))
}

async fn show_collection(
    State(state): State<Arc<SharedState>>,
    Path(id): Path<i64>,
) -> Result<Json<Collection>, error::Error> {
    let mut builder: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT L.id, L.name, L.description FROM seedcollections L WHERE id=");
    builder.push_bind(id);
    let mut collection: Collection = builder.build_query_as().fetch_one(&state.dbpool).await?;
    let mut builder = sample::build_query(Some(id), None);
    collection.samples = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(Json(collection))
}

#[derive(Deserialize)]
struct ModifyProps {
    name: Option<String>,
    description: Option<String>,
}

async fn modify_collection(
    State(state): State<Arc<SharedState>>,
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
    State(state): State<Arc<SharedState>>,
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
    State(state): State<Arc<SharedState>>,
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

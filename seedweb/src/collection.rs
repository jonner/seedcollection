use std::sync::Arc;

use crate::{error, state::SharedState};
use anyhow::Result;
use axum::{
    extract::{Path, State},
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::{collection::Collection, sample};
use sqlx::{QueryBuilder, Sqlite};

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_collections))
        .route("/:id", get(show_collection))
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

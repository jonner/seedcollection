use crate::db;
use crate::error;
use anyhow::Result;
use axum::{
    extract::Path,
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::collection;
use libseed::collection::Collection;
use serde::{Deserialize, Serialize};

pub fn router() -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/list", get(list_handler))
}

async fn root_handler() -> Html<String> {
    Html("Collections".to_string())
}

async fn list_handler() -> Result<Json<Vec<Collection>>, error::Error> {
    let pool = db::pool().await?;
    let collections: Vec<Collection> =
        sqlx::query_as("SELECT L.id, L.name, L.description FROM seedcollections L")
            .fetch_all(&pool)
            .await?;
    Ok(Json(collections))
}

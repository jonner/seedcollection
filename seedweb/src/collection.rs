use crate::db;
use crate::error;
use anyhow::Result;
use axum::{
    extract::Path,
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::collection::Collection;
use libseed::sample;
use sqlx::QueryBuilder;
use sqlx::Sqlite;

pub fn router() -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/list", get(list_handler))
        .route("/:id", get(show_handler))
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

async fn show_handler(Path(id): Path<i64>) -> Result<Json<Collection>, error::Error> {
    let pool = db::pool().await?;
    let mut builder: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT L.id, L.name, L.description FROM seedcollections L WHERE id=");
    builder.push_bind(id);
    let mut collection: Collection = builder.build_query_as().fetch_one(&pool).await?;
    let mut builder = sample::build_query(Some(id));
    collection.samples = builder.build_query_as().fetch_all(&pool).await?;
    Ok(Json(collection))
}

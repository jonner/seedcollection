use crate::db;
use crate::error;
use anyhow::Result;
use axum::{
    extract::Query,
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::sample;
use libseed::sample::Sample;
use serde::{Deserialize, Serialize};

pub fn router() -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/list", get(list_handler))
}

async fn root_handler() -> Html<String> {
    Html("Samples".to_string())
}

async fn list_handler() -> Result<Json<Vec<Sample>>, error::Error> {
    let pool = db::pool().await?;
    let mut builder = sample::build_query(None);
    let locations: Vec<Sample> = builder.build_query_as().fetch_all(&pool).await?;
    Ok(Json(locations))
}

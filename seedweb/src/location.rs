use crate::db;
use crate::error;
use anyhow::Result;
use axum::{
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::location;
use libseed::location::Location;

pub fn router() -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/list", get(list_handler))
}

async fn root_handler() -> Html<String> {
    Html("Locations".to_string())
}

async fn list_handler() -> Result<Json<Vec<Location>>, error::Error> {
    let pool = db::pool().await?;
    let locations: Vec<location::Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations",
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(locations))
}

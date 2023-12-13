use crate::{error, state::SharedState};
use anyhow::Result;
use axum::{
    extract::{Path, State},
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::location::{self, Location};
use std::sync::Arc;

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root_handler))
        .route("/list", get(list_handler))
        .route("/:id", get(show_handler))
}

async fn root_handler() -> Html<String> {
    Html("Locations".to_string())
}

async fn list_handler(
    State(state): State<Arc<SharedState>>,
) -> Result<Json<Vec<Location>>, error::Error> {
    let locations: Vec<location::Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations",
    )
    .fetch_all(&state.dbpool)
    .await?;
    Ok(Json(locations))
}

async fn show_handler(
    Path(id): Path<i64>,
    State(state): State<Arc<SharedState>>,
) -> Result<Json<Location>, error::Error> {
    let location: Location = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations WHERE locid=?",
    ).bind(id)
    .fetch_one(&state.dbpool)
    .await?;
    Ok(Json(location))
}

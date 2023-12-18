use crate::CustomKey;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use axum_template::RenderHtml;
use libseed::location::{self, Location};
use minijinja::context;

use crate::{error, state::SharedState};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_locations))
        .route("/new", post(add_location))
        .route("/:id", get(show_location))
}

async fn root() -> impl IntoResponse {
    "Locations"
}

async fn list_locations(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let locations: Vec<location::Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations",
    )
    .fetch_all(&state.dbpool)
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(locations => locations),
    ))
}

async fn add_location() -> impl IntoResponse {
    todo!()
}

async fn show_location(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let loc: Location = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations WHERE locid=?"
    ).bind(id)
    .fetch_one(&state.dbpool)
    .await?;
    Ok(RenderHtml(key, state.tmpl, context!(location => loc)))
}

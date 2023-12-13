use crate::{error, state::SharedState};
use anyhow::Result;
use axum::{
    extract::{Path, State},
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::sample::{self, Sample};
use std::sync::Arc;

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_samples))
        .route("/:id", get(show_sample))
}

async fn root(State(_state): State<Arc<SharedState>>) -> Html<String> {
    Html("Samples".to_string())
}

async fn list_samples(
    State(state): State<Arc<SharedState>>,
) -> Result<Json<Vec<Sample>>, error::Error> {
    let mut builder = sample::build_query(None, None);
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(Json(samples))
}

async fn show_sample(
    State(state): State<Arc<SharedState>>,
    Path(id): Path<i64>,
) -> Result<Json<Sample>, error::Error> {
    let mut builder = sample::build_query(None, Some(id));
    let sample: Sample = builder.build_query_as().fetch_one(&state.dbpool).await?;
    Ok(Json(sample))
}

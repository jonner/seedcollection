use crate::{error, state::SharedState};
use anyhow::Result;
use axum::{
    extract::State,
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::sample::{self, Sample};
use std::sync::Arc;

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root_handler))
        .route("/list", get(list_handler))
}

async fn root_handler(State(_state): State<Arc<SharedState>>) -> Html<String> {
    Html("Samples".to_string())
}

async fn list_handler(
    State(state): State<Arc<SharedState>>,
) -> Result<Json<Vec<Sample>>, error::Error> {
    let mut builder = sample::build_query(None);
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(Json(samples))
}

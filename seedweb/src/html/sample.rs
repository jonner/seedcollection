use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use axum_template::RenderHtml;
use libseed::sample::{self, Sample};
use minijinja::context;

use crate::{error, state::SharedState, CustomKey};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_samples))
        .route("/new", post(add_sample))
        .route(
            "/:id",
            get(show_sample).put(modify_sample).delete(delete_sample),
        )
}

async fn root() -> impl IntoResponse {
    "Samples"
}

async fn list_samples(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(None, None);
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(RenderHtml(key, state.tmpl, context!(samples => samples)))
}

async fn add_sample() -> impl IntoResponse {
    todo!()
}

async fn show_sample(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(None, Some(id));
    let sample: Sample = builder.build_query_as().fetch_one(&state.dbpool).await?;
    Ok(RenderHtml(key, state.tmpl, context!(sample => sample)))
}

async fn modify_sample() -> impl IntoResponse {
    todo!()
}

async fn delete_sample() -> impl IntoResponse {
    todo!()
}

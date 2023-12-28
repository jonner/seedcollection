use crate::{error, state::SharedState, CustomKey};
use axum::{extract::State, response::IntoResponse, routing::get, Router};
use axum_template::RenderHtml;
use minijinja::context;

mod auth;
mod collection;
mod location;
mod sample;
mod taxonomy;

pub fn router() -> Router<SharedState> {
    Router::new()
        .nest("/auth/", auth::router())
        .nest("/collection/", collection::router())
        .nest("/location/", location::router())
        .nest("/sample/", sample::router())
        .nest("/taxonomy/", taxonomy::router())
        .route("/", get(root))
}

async fn root(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl, context!()))
}

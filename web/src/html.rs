use crate::{app_url, auth::SqliteAuthBackend, error, state::AppState, CustomKey};
use axum::{extract::State, response::IntoResponse, routing::get, Router};
use axum_login::login_required;
use axum_template::RenderHtml;
use minijinja::context;

mod auth;
mod collection;
mod location;
mod sample;
mod taxonomy;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/collection/", collection::router())
        .nest("/location/", location::router())
        .nest("/sample/", sample::router())
        .nest("/taxonomy/", taxonomy::router())
        /* Anything above here is only available to logged-in users */
        .route_layer(login_required!(
            SqliteAuthBackend,
            login_url = app_url("/auth/login")
        ))
        .route("/", get(root))
        .nest("/auth/", auth::router())
}

async fn root(
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!()))
}

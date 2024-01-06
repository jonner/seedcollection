use crate::{
    app_url,
    auth::{AuthSession, SqliteAuthBackend},
    error,
    state::AppState,
    TemplateKey,
};
use axum::{extract::State, response::IntoResponse, routing::get, Router};
use axum_login::login_required;
use axum_template::RenderHtml;
use minijinja::context;

mod auth;
mod collection;
mod info;
mod location;
mod sample;
mod taxonomy;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/collection/", collection::router())
        .nest("/location/", location::router())
        .nest("/sample/", sample::router())
        .nest("/taxonomy/", taxonomy::router())
        .nest("/info/", info::router())
        /* Anything above here is only available to logged-in users */
        .route_layer(login_required!(
            SqliteAuthBackend,
            login_url = &app_url("/auth/login")
        ))
        .route("/", get(root))
        .nest("/auth/", auth::router())
}

async fn root(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user),
    ))
}
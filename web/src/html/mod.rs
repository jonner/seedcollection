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

mod allocation;
mod auth;
mod info;
mod project;
mod sample;
mod source;
mod taxonomy;
mod user;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/info/", info::router())
        .nest("/project/", project::router())
        .nest("/sample/", sample::router())
        .nest("/source/", source::router())
        .nest("/taxonomy/", taxonomy::router())
        .nest("/user/", user::router())
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
    tracing::info!("root");
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user),
    ))
}

use crate::{auth::AuthSession, error, state::AppState, TemplateKey};
use axum::{
    extract::{OriginalUri, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
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

async fn login_required(
    State(state): State<AppState>,
    auth: AuthSession,
    OriginalUri(uri): OriginalUri,
    request: Request,
    next_layer: Next,
) -> Response {
    if auth.user.is_some() {
        next_layer.run(request).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            RenderHtml(
                "auth_login.html",
                state.tmpl.clone(),
                context!(
                next => uri.to_string(),
                ),
            ),
        )
            .into_response()
    }
}

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/info/", info::router())
        .nest("/project/", project::router())
        .nest("/sample/", sample::router())
        .nest("/source/", source::router())
        .nest("/taxonomy/", taxonomy::router())
        .nest("/user/", user::router())
        /* Anything above here is only available to logged-in users */
        .route_layer(middleware::from_fn_with_state(state, login_required))
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

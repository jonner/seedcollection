use crate::{
    TemplateKey,
    auth::AuthSession,
    error::Error,
    state::AppState,
    util::{FlashMessage, FlashMessageKind},
};
use axum::{
    Router,
    extract::{OriginalUri, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use axum_template::RenderHtml;
use minijinja::context;
use serde::Serialize;

mod allocation;
mod auth;
mod info;
mod project;
mod sample;
mod source;
mod taxonomy;
#[cfg(test)]
mod tests;
mod user;

pub(crate) fn flash_message(
    state: std::sync::Arc<crate::SharedState>,
    kind: FlashMessageKind,
    msg: String,
) -> impl IntoResponse {
    RenderHtml(
        "_flash_messages.html.j2",
        state.tmpl.clone(),
        context!(messages => &[FlashMessage {
            kind,
            msg: msg.into(),
        }]),
    )
}

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
                "auth_login.html.j2",
                state.tmpl.clone(),
                context!(
                next => uri.to_string(),
                ),
            ),
        )
            .into_response()
    }
}

pub(crate) fn router(state: AppState) -> Router<AppState> {
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
) -> Result<impl IntoResponse, Error> {
    tracing::info!("root");
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user),
    ))
}

/// A utility type for specifying an option for sorting
#[derive(Serialize)]
struct SortOption<T: Serialize> {
    name: String,
    code: T,
    selected: bool,
}

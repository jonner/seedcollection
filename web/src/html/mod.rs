use crate::{
    TemplateKey,
    auth::AuthSession,
    error::Error,
    state::AppState,
    util::{FlashMessage, app_url},
};
use axum::{
    Router,
    extract::{OriginalUri, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use libseed::core::query::SortOrder;
use minijinja::context;
use serde::Serialize;

mod allocation;
pub(crate) mod auth;
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
    msg: FlashMessage,
) -> impl IntoResponse {
    state.render_template("_flash_messages.html.j2", context!(messages => &[msg]))
}

async fn login_required(
    State(state): State<AppState>,
    auth: AuthSession,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    request: Request,
    next_layer: Next,
) -> Response {
    if auth.user.is_some() {
        next_layer.run(request).await
    } else {
        // FIXME: this might not be the correct url to redirect to after login.
        // For example, if the request was a htmx sub-request, the `uri` variable will
        // point to this subquery and this is not the page that the user is currently
        // viewing in the browser, so redirecting to that subquery page might not behave
        // as expected.
        let next_params = serde_urlencoded::to_string([(
            "next",
            uri.path_and_query()
                .map(|pq| pq.as_str())
                .unwrap_or_default(),
        )])
        .unwrap_or_default();
        let mut login_url = app_url("/auth/login");
        if !next_params.is_empty() {
            login_url.push('?');
            login_url.push_str(&next_params);
        }
        if headers.get("HX-Request").is_some() {
            (
                StatusCode::UNAUTHORIZED,
                [
                    ("HX-Retarget", "#flash-messages"),
                    ("HX-Reswap", "innerHTML"),
                ],
                flash_message(
                    state,
                    FlashMessage::Error(format!(
                        "This action requires an authenticated user. Please [log in]({login_url})"
                    )),
                ),
            )
                .into_response()
        } else {
            (
                StatusCode::UNAUTHORIZED,
                state.render_template(
                    "auth_login.html.j2",
                    context!(
                    next => uri.to_string(),
                    ),
                ),
            )
                .into_response()
        }
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
    Ok(state.render_template(key, context!(user => auth.user)))
}

#[derive(Serialize)]
pub(crate) struct FilterSortSpec<T: Serialize> {
    filter: String,
    sort_fields: Vec<FilterSortOption<T>>,
    sort_dirs: Vec<FilterSortOption<SortOrder>>,
    additional_filters: Vec<FilterSortOption<String>>,
}

/// A utility type for specifying an option for sorting
#[derive(Serialize)]
pub(crate) struct FilterSortOption<T: Serialize> {
    name: String,
    value: T,
    selected: bool,
}

/// A utility function for creating the sort order options
pub(crate) fn sort_dirs(selected: SortOrder) -> Vec<FilterSortOption<SortOrder>> {
    <SortOrder as strum::IntoEnumIterator>::iter()
        .map(|val| FilterSortOption {
            name: val.to_string(),
            value: val,
            selected: val == selected,
        })
        .collect()
}

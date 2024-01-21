use crate::{auth::AuthSession, error::Error, state::AppState, TemplateKey};
use axum::{extract::State, response::IntoResponse, routing::get, Router};
use axum_template::RenderHtml;
use minijinja::context;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(show_profile))
        .route("/me/edit", get(show_edit_profile))
}

async fn show_profile(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user),
    ))
}

async fn show_edit_profile(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user),
    ))
}

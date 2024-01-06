use axum::{extract::State, response::IntoResponse, routing::get, Router};
use axum_template::RenderHtml;
use libseed::taxonomy::Germination;
use minijinja::context;

use crate::{auth::AuthSession, error, state::AppState, TemplateKey};

pub fn router() -> Router<AppState> {
    Router::new().route("/germination", get(germination))
}

async fn germination(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let germination = sqlx::query_as!(Germination, "SELECT * FROM sc_germination_codes")
        .fetch_all(&state.dbpool)
        .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 germination => germination),
    ))
}

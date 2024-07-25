use axum::{extract::State, response::IntoResponse, routing::get, Router};
use axum_template::RenderHtml;
use libseed::taxonomy::Germination;
use minijinja::context;

use crate::{auth::SqliteUser, error, state::AppState, TemplateKey};

pub fn router() -> Router<AppState> {
    Router::new().route("/germination", get(germination))
}

async fn germination(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let germination = Germination::load_all(&state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 germination => germination),
    ))
}

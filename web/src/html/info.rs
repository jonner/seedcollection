use crate::{TemplateKey, auth::SqliteUser, error::Error, state::AppState};
use axum::{Router, extract::State, response::IntoResponse, routing::get};
use libseed::taxonomy::Germination;
use minijinja::context;

pub(crate) fn router() -> Router<AppState> {
    Router::new().route("/germination", get(germination))
}

async fn germination(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let germination = Germination::load_all(&state.db).await?;
    Ok(state.render_template(
        key,
        context!(user => user,
                 germination => germination),
    ))
}

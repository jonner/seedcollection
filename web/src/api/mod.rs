use crate::state::AppState;
use axum::{response::Html, routing::get, Router};

mod collection;
mod sample;
mod source;
mod taxonomy;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .nest("/collection/", collection::router())
        .nest("/source/", source::router())
        .nest("/sample/", sample::router())
        .nest("/taxonomy/", taxonomy::router())
}

async fn root() -> Html<String> {
    Html("seedweb API root here".to_string())
}

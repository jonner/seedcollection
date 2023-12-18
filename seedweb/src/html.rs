use crate::state::SharedState;
use axum::{response::Html, routing::get, Router};

mod collection;
mod location;
mod sample;
mod taxonomy;

pub fn router() -> Router<SharedState> {
    Router::new()
        .nest("/collection/", collection::router())
        .nest("/location/", location::router())
        .nest("/sample/", sample::router())
        .nest("/taxonomy/", taxonomy::router())
        .route("/", get(root))
}

async fn root() -> Html<&'static str> {
    Html("This is the seedcollection app")
}

use axum::Router;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use libseed::collection::Collection;
use std::sync::Arc;

use crate::{error, state::SharedState};

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_collections))
        .route("/new", post(add_collection))
        .route(
            "/:id",
            get(show_collection)
                .put(modify_collection)
                .delete(delete_collection),
        )
}

async fn root() -> impl IntoResponse {
    "Collections"
}

async fn list_collections(
    State(state): State<Arc<SharedState>>,
) -> Result<Html<String>, error::Error> {
    let collections: Vec<Collection> =
        sqlx::query_as("SELECT L.id, L.name, L.description FROM seedcollections L")
            .fetch_all(&state.dbpool)
            .await?;
    let mut output = r#"
     <!DOCTYPE html>
    <html>
    <head>
    <script src="https://unpkg.com/htmx.org@1.9.9" integrity="sha384-QFjmbokDn2DjBjq+fM+8LUIVrAgqcNW2s0PjAxHETgRn9l4fvX31ZxDxvwQnyMOX" crossorigin="anonymous"></script>
    </head>
    <body>
    <h1>Collections</h1>
    <div id="update-results"></div>
    "#.to_string();
    for collection in &collections {
        output.push_str(&format!(r#"
        <form hx-put='/api/1/collection/{0}' hx-trigger='submit' hx-target='#update-results' id='cf-{}'>
        {0}
        <input type="text" form="cf-{0}" name="name" value="{}">
        <input type="text" form="cf-{0}" name="description" value="{}">
        <input type="submit" form="cf-{0}" value="Update">
        </form>
        "#,
            collection.id,
            collection.name,
            collection.description.as_ref().unwrap_or(&"".to_string())
        ));
    }
    output.push_str("
        </body>
        </html>",
    );
    Ok(Html(output))
}

async fn add_collection() -> impl IntoResponse {
    "Add collection"
}

async fn show_collection(Path(id): Path<i64>) -> impl IntoResponse {
    format!("Show collection {}", id)
}

async fn modify_collection() -> impl IntoResponse {
    "Modify collection"
}

async fn delete_collection() -> impl IntoResponse {
    "Delete collection"
}

use axum::Router;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::{get, post},
};
use libseed::collection::Collection;
use libseed::sample;
use sqlx::{QueryBuilder, Sqlite};
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
    <ul>
    "#.to_string();
    for c in &collections {
        output.push_str(&format!(
            r#"
        <li>{0}: <a href="{0}">{1}</a></li>
        "#,
            c.id, c.name,
        ));
    }
    output.push_str("</ul>");
    output.push_str(
        "
        </body>
        </html>",
    );
    Ok(Html(output))
}

async fn add_collection() -> impl IntoResponse {
    "Add collection"
}

async fn show_collection(
    Path(id): Path<i64>,
    State(state): State<Arc<SharedState>>,
) -> Result<Html<String>, error::Error> {
    let mut c: Collection =
        sqlx::query_as("SELECT L.id, L.name, L.description FROM seedcollections L WHERE id=?")
            .bind(id)
            .fetch_one(&state.dbpool)
            .await?;
    let mut builder = sample::build_query(Some(id), None);
    c.samples = builder.build_query_as().fetch_all(&state.dbpool).await?;

    let mut output = format!(
        r#"
     <!DOCTYPE html>
    <html>
    <head>
    <script src="https://unpkg.com/htmx.org@1.9.9" integrity="sha384-QFjmbokDn2DjBjq+fM+8LUIVrAgqcNW2s0PjAxHETgRn9l4fvX31ZxDxvwQnyMOX" crossorigin="anonymous"></script>
    </head>
    <body>
    <h1>Collection Details</h1>
        <form hx-put="/api/v1/collection/{0}" hx-swap="none">
        <p>ID: {0}</p>
        <div><input type="text" name="name" value="{1}"></div>
        <div><textarea name="description">{2}</textarea></div>
        <div><input type="submit" value="Update"></div>
        </form>
        <table>
        <tr>
        <th>ID</th>
        <th>Taxon</th>
        <th>Location</th>
        </tr>
        "#,
        c.id,
        c.name,
        c.description.unwrap_or("".to_string()),
    );
    for s in c.samples {
        output.push_str(&format!(
            r#"
        <tr>
        <td><a href="/app/sample/{0}">{0}</a></td>
        <td>{1}</td>
        <td>{2}</td>
        </tr>"#,
            s.id, s.taxon.complete_name, s.location.name,
        ));
    }
    output.push_str(
        "
        </body>
        </html>",
    );
    Ok(Html(output))
}

async fn modify_collection() -> impl IntoResponse {
    "Modify collection"
}

async fn delete_collection() -> impl IntoResponse {
    "Delete collection"
}

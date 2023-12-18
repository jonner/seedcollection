use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use libseed::taxonomy::{self, Taxon};
use std::sync::Arc;

use crate::{error, state::SharedState};

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_taxa))
        .route("/:id", get(show_taxon))
}

async fn root() -> impl IntoResponse {
    "Taxonomy"
}

async fn list_taxa(State(state): State<Arc<SharedState>>) -> Result<Html<String>, error::Error> {
    let taxa: Vec<Taxon> = taxonomy::build_query(None, None, None, None, None, false)
        .build_query_as()
        .fetch_all(&state.dbpool)
        .await?;
    let mut output = format!(
        r#"
    <!DOCTYPE html>
    <html>
    <head>
    <script src="https://unpkg.com/htmx.org@1.9.9" integrity="sha384-QFjmbokDn2DjBjq+fM+8LUIVrAgqcNW2s0PjAxHETgRn9l4fvX31ZxDxvwQnyMOX" crossorigin="anonymous"></script>
    </head>
    <body>
    <h1>Taxonomy</h1>
    <ul>
    "#
    );
    for t in taxa {
        output.push_str(&format!(
            "<li><a href='./{}'>{}</a></li>",
            t.id, t.complete_name
        ));
    }
    output.push_str(
        "</ul>
    </body>
    </html>",
    );
    Ok(Html(output))
}

async fn show_taxon(
    State(state): State<Arc<SharedState>>,
    Path(id): Path<i64>,
) -> Result<Html<String>, error::Error> {
    let taxon: Taxon = taxonomy::build_query(Some(id), None, None, None, None, false)
        .build_query_as()
        .fetch_one(&state.dbpool)
        .await?;
    let output = format!(
        r#"
    <!DOCTYPE html>
    <html>
    <head>
    <script src="https://unpkg.com/htmx.org@1.9.9" integrity="sha384-QFjmbokDn2DjBjq+fM+8LUIVrAgqcNW2s0PjAxHETgRn9l4fvX31ZxDxvwQnyMOX" crossorigin="anonymous"></script>
    </head>
    <body>
    <h1>Taxon Details</h1>
    <bdl>
    <dt>ID</dt>
    <dd>{}</dd>
    <dt>Name</dt>
    <dd>{}</dd>
    <dt>name1</dt>
    <dd>{}</dd>
    <dt>name2</dt>
    <dd>{}</dd>
    <dt>name3</dt>
    <dd>{}</dd>
    <dt>Vernacular Names</dt>
    <dd><ul>{}</ul></dd>
    <dt>Minnesota Status</dt>
    <dd>{}</dd>
    </dl>
    </body>
    </html>"#,
        taxon.id,
        taxon.complete_name,
        taxon.name1.unwrap_or("".to_string()),
        taxon.name2.unwrap_or("".to_string()),
        taxon.name3.unwrap_or("".to_string()),
        taxon
            .vernaculars
            .iter()
            .map(|n| format!("<li>{}</li>", n))
            .collect::<Vec<String>>()
            .join("\n"),
        taxon
            .native_status
            .map(|x| x.to_string())
            .unwrap_or("".to_string()),
    );
    Ok(Html(output))
}

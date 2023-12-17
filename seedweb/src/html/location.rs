use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use libseed::location::{self, Location};
use std::sync::Arc;

use crate::{error, state::SharedState};

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_locations))
        .route("/new", post(add_location))
        .route("/:id", get(show_location))
}

async fn root() -> impl IntoResponse {
    "Locations"
}

async fn list_locations(
    State(state): State<Arc<SharedState>>,
) -> Result<Html<String>, error::Error> {
    let locations: Vec<location::Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations",
    )
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
    <h1>Locations</h1>
    <ul>
    "#
    );
    for l in locations {
        output.push_str(&format!("<li><a href='../{}'>{}</a></li>", l.id, l.name));
    }
    output.push_str(
        "</ul>
    </body>
    </html>",
    );
    Ok(Html(output))
}

async fn add_location() -> impl IntoResponse {
    todo!()
}

async fn show_location(
    State(state): State<Arc<SharedState>>,
    Path(id): Path<i64>,
) -> Result<Html<String>, error::Error> {
    let loc: Location = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations WHERE locid=?"
    ).bind(id)
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
    <h1>Location Details</h1>
    <form hx-put="/api/v1/location/{}" hx-swap="none">
    <bdl>
    <dt>ID</dt>
    <dd>{}</dd>
    <dt>Name</dt>
    <dd><input type="text" value="{}"></dd>
    <dt>Description</dt>
    <dd><textarea name="description">{}</textarea></dd>
    </dl>
    <input type="submit" value="Update"/>
    </form>
    </body>
    </html>"#,
        loc.id,
        loc.id,
        loc.name,
        loc.description.unwrap_or("".to_string()),
    );
    Ok(Html(output))
}

use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use libseed::sample::{self, Sample};
use std::sync::Arc;

use crate::{error, state::SharedState};

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_samples))
        .route("/new", post(add_sample))
        .route(
            "/:id",
            get(show_sample).put(modify_sample).delete(delete_sample),
        )
}

async fn root() -> impl IntoResponse {
    "Samples"
}

async fn list_samples() -> impl IntoResponse {
    todo!()
}

async fn add_sample() -> impl IntoResponse {
    todo!()
}

async fn show_sample(
    State(state): State<Arc<SharedState>>,
    Path(id): Path<i64>,
) -> Result<Html<String>, error::Error> {
    let mut builder = sample::build_query(None, Some(id));
    let sample: Sample = builder.build_query_as().fetch_one(&state.dbpool).await?;
    let output = format!(
        r#"
    <!DOCTYPE html>
    <html>
    <head>
    <script src="https://unpkg.com/htmx.org@1.9.9" integrity="sha384-QFjmbokDn2DjBjq+fM+8LUIVrAgqcNW2s0PjAxHETgRn9l4fvX31ZxDxvwQnyMOX" crossorigin="anonymous"></script>
    </head>
    <body>
    <h1>Sample Details</h1>
    <form hx-put="/api/v1/sample/{}">
    <bdl>
    <dt>ID</dt>
    <dd>{}</dd>
    <dt>Taxon</dt>
    <dd><a href="/app/taxon/{}">{}</a></dd>
    <dt>Location</dt>
    <dd><a href="/app/location/{}">{}</a></dd>
    <dt>Month</dt>
    <dd><input type="number" name="month" value="{}"/></dd>
    <dt>Year</dt>
    <dd><input type="number" name="year" value="{}"/></dd>
    <dt>Quantity</dt>
    <dd><input type="number" name="quantity" value="{}"/></dd>
    <dt>Notes</dt>
    <dd><textarea name="notes">{}</textarea></dd>
    </dl>
    <input type="submit" value="Update"/>
    </form>
    </body>
    </html>"#,
        sample.id,
        sample.id,
        sample.taxon.id,
        sample.taxon.complete_name,
        sample.location.id,
        sample.location.name,
        sample
            .month
            .map(|x| x.to_string())
            .unwrap_or("".to_string()),
        sample.year.map(|x| x.to_string()).unwrap_or("".to_string()),
        sample
            .quantity
            .map(|x| x.to_string())
            .unwrap_or("".to_string()),
        sample.notes.unwrap_or("".to_string()),
    );
    Ok(Html(output))
}

async fn modify_sample() -> impl IntoResponse {
    todo!()
}

async fn delete_sample() -> impl IntoResponse {
    todo!()
}

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use axum_template::RenderHtml;
use libseed::taxonomy::{self, Taxon};
use minijinja::context;

use crate::{error, state::SharedState, CustomKey};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_taxa))
        .route("/:id", get(show_taxon))
}

async fn root() -> impl IntoResponse {
    "Taxonomy"
}

async fn list_taxa(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let taxa: Vec<Taxon> = taxonomy::build_query(None, None, None, None, None, false)
        .build_query_as()
        .fetch_all(&state.dbpool)
        .await?;
    Ok(RenderHtml(key, state.tmpl, context!(taxa => taxa)))
}

async fn show_taxon(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let hierarchy = taxonomy::fetch_taxon_hierarchy(id, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(taxon => hierarchy[0], hierarchy => hierarchy),
    ))
}

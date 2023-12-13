use crate::{error, state::SharedState};
use axum::{
    extract::{Query, State},
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::taxonomy::{self, Rank, Taxon};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root_handler))
        .route("/find", get(find_handler))
}

async fn root_handler() -> Html<String> {
    Html("Taxonomy".to_string())
}

#[derive(Deserialize, Serialize)]
struct TaxonomyFindParams {
    id: Option<i64>,
    rank: Option<Rank>,
    genus: Option<String>,
    species: Option<String>,
    any: Option<String>,
    minnesota: Option<bool>,
}

async fn find_handler(
    State(state): State<Arc<SharedState>>,
    Query(params): Query<TaxonomyFindParams>,
) -> Result<Json<Vec<Taxon>>, error::Error> {
    // FIXME: share db connections?
    let t = taxonomy::build_query(
        params.id,
        params.rank,
        params.genus,
        params.species,
        params.any,
        params.minnesota.unwrap_or(false),
    )
    .build_query_as::<Taxon>()
    .fetch_all(&state.dbpool)
    .await?;
    Ok(Json(t))
}

use crate::{error, state::SharedState};
use axum::{
    extract::{Path, Query, State},
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::taxonomy::{self, Rank, Taxon};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use strum::IntoEnumIterator;

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root))
        .route("/find", get(find_taxa))
        .route("/ranks", get(ranks))
        .route("/:id", get(show_taxon))
}

async fn root() -> Html<String> {
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

async fn find_taxa(
    State(state): State<Arc<SharedState>>,
    Query(params): Query<TaxonomyFindParams>,
) -> Result<Json<Vec<Taxon>>, error::Error> {
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

async fn show_taxon(
    State(state): State<Arc<SharedState>>,
    Path(id): Path<i64>,
) -> Result<Json<Taxon>, error::Error> {
    let t = taxonomy::build_query(Some(id), None, None, None, None, false)
        .build_query_as::<Taxon>()
        .fetch_one(&state.dbpool)
        .await?;
    Ok(Json(t))
}

#[derive(Serialize)]
struct RanksResponse {
    ranks: Vec<String>,
}

async fn ranks() -> Result<Json<RanksResponse>, error::Error> {
    let mut ranks = Vec::new();
    for val in taxonomy::Rank::iter() {
        ranks.push(val.to_string());
    }
    Ok(Json(RanksResponse { ranks }))
}
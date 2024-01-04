use crate::{error, state::AppState};
use axum::{
    extract::{Path, Query, State},
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::taxonomy::{self, filter_by, Rank, Taxon};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

pub fn router() -> Router<AppState> {
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
    State(state): State<AppState>,
    Query(params): Query<TaxonomyFindParams>,
) -> Result<Json<Vec<Taxon>>, error::Error> {
    let t = Taxon::query(
        filter_by(
            params.id,
            params.rank,
            params.genus,
            params.species,
            params.any,
            params.minnesota,
        ),
        None,
        &state.dbpool,
    )
    .await?;
    Ok(Json(t))
}

async fn show_taxon(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Taxon>, error::Error> {
    let t = Taxon::fetch(id, &state.dbpool).await?;
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

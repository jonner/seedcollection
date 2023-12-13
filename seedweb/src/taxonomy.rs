use crate::error;
use anyhow::Result;
use axum::{
    extract::Query,
    response::{Html, Json},
    routing::get,
    Router,
};
use libseed::taxonomy;
use libseed::taxonomy::Rank;
use libseed::taxonomy::Taxon;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

pub fn router() -> Router {
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

async fn dbpool() -> Result<SqlitePool> {
    Ok(SqlitePool::connect("sqlite://seedcollection.sqlite").await?)
}

async fn find_handler(
    Query(params): Query<TaxonomyFindParams>,
) -> Result<Json<Vec<Taxon>>, error::Error> {
    // FIXME: share db connections?
    let dbpool = dbpool().await?;
    let t = taxonomy::build_query(
        params.id,
        params.rank,
        params.genus,
        params.species,
        params.any,
        params.minnesota.unwrap_or(false),
    )
    .build_query_as::<Taxon>()
    .fetch_all(&dbpool)
    .await?;
    Ok(Json(t))
}

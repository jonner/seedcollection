use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use axum_template::RenderHtml;
use libseed::taxonomy::{
    self, any_filter, filter_by, CompoundFilterCondition, FilterOperation, FilterQueryBuilder,
    Taxon,
};
use minijinja::context;
use serde::Deserialize;

use crate::{error, state::SharedState, CustomKey};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_taxa))
        .route("/:id", get(show_taxon))
        .route("/quickfind", get(quickfind))
}

async fn root() -> impl IntoResponse {
    "Taxonomy"
}

async fn list_taxa(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let taxa: Vec<Taxon> = taxonomy::build_query(None, None)
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
    let children = taxonomy::fetch_children(id, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(taxon => hierarchy[0], parents => hierarchy, children => children),
    ))
}

#[derive(Deserialize)]
struct QuickfindParams {
    taxon: String,
}

async fn quickfind(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Query(QuickfindParams { taxon }): Query<QuickfindParams>,
) -> Result<impl IntoResponse, error::Error> {
    let taxa: Vec<Taxon> = match taxon.is_empty() {
        true => Vec::new(),
        false => {
            let parts = taxon.split(" ");
            let mut subfilters: Vec<Box<dyn FilterQueryBuilder>> = Vec::new();
            for part in parts {
                subfilters.push(Box::new(any_filter(part)));
            }
            let filter = CompoundFilterCondition::new(FilterOperation::And, subfilters);
            taxonomy::build_query(Some(Box::new(filter)), Some(20))
                .build_query_as()
                .fetch_all(&state.dbpool)
                .await?
        }
    };
    Ok(RenderHtml(key, state.tmpl, context!(taxa => taxa)))
}

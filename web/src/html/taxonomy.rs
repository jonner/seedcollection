use axum::{
    extract::{Path, Query, Request, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use axum_template::RenderHtml;
use libseed::taxonomy::{
    self, any_filter, CompoundFilterCondition, FilterField, FilterOperation, FilterQueryBuilder,
    LimitSpec, Rank, Taxon,
};
use log::debug;
use minijinja::context;
use serde::Deserialize;
use sqlx::Row;

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

const PAGE_SIZE: i32 = 100;
#[derive(Deserialize)]
struct ListParams {
    rank: Option<Rank>,
    page: Option<i32>,
}

async fn list_taxa(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Query(params): Query<ListParams>,
    req: Request,
) -> Result<impl IntoResponse, error::Error> {
    let rank = match params.rank {
        Some(r) => r,
        None => Rank::Species,
    };
    let pg = match params.page {
        Some(n) => n,
        None => 1,
    };
    let row = taxonomy::count_query(Some(Box::new(FilterField::Rank(rank.clone()))))
        .build()
        .fetch_one(&state.dbpool)
        .await?;
    let count = row.try_get::<i32, _>("count")?;
    let total_pages = (count + PAGE_SIZE - 1) / PAGE_SIZE;
    let taxa: Vec<Taxon> = taxonomy::build_query(
        Some(Box::new(FilterField::Rank(rank))),
        Some(LimitSpec(PAGE_SIZE, Some(PAGE_SIZE * (pg - 1)))),
    )
    .build_query_as()
    .fetch_all(&state.dbpool)
    .await?;
    debug!("req={:?}", req);
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(taxa => taxa, page => pg, total_pages => total_pages, request_uri => req.uri().to_string()),
    ))
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
            taxonomy::build_query(Some(Box::new(filter)), Some(LimitSpec(200, None)))
                .build_query_as()
                .fetch_all(&state.dbpool)
                .await?
        }
    };
    Ok(RenderHtml(key, state.tmpl, context!(taxa => taxa)))
}

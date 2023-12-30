use axum::{
    extract::{Path, Query, Request, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use axum_template::RenderHtml;
use libseed::{
    sample::{self, Filter, Sample},
    taxonomy::{
        self, any_filter, CompoundFilterCondition, FilterField, FilterOperation,
        FilterQueryBuilder, LimitSpec, Rank, Taxon,
    },
};
use minijinja::context;
use serde::Deserialize;
use sqlx::Row;
use tracing::debug;

use crate::{auth::AuthSession, error, state::AppState, CustomKey};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_taxa))
        .route("/:id", get(show_taxon))
        .route("/quickfind", get(quickfind))
}

async fn root(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user),
    ))
}

const PAGE_SIZE: i32 = 100;
#[derive(Deserialize)]
struct ListParams {
    rank: Option<Rank>,
    page: Option<i32>,
}

async fn list_taxa(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
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
        state.tmpl.clone(),
        context!(user => auth.user,
                 taxa => taxa,
                 page => pg,
                 total_pages => total_pages,
                 request_uri => req.uri().to_string()),
    ))
}

async fn show_taxon(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let hierarchy = taxonomy::fetch_taxon_hierarchy(id, &state.dbpool).await?;
    let children = taxonomy::fetch_children(id, &state.dbpool).await?;
    let samples: Vec<Sample> = sample::build_query(Some(Filter::Taxon(id)))
        .build_query_as()
        .fetch_all(&state.dbpool)
        .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 taxon => hierarchy[0],
                 parents => hierarchy,
                 children => children,
                 samples => samples),
    ))
}

#[derive(Deserialize)]
struct QuickfindParams {
    taxon: String,
}

async fn quickfind(
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
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
    Ok(RenderHtml(key, state.tmpl.clone(), context!(taxa => taxa)))
}

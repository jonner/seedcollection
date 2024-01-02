use axum::{
    extract::{Path, Query, Request, State},
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    sample::{self, Filter, Sample},
    taxonomy::{
        self, any_filter, CompoundFilterCondition, FilterField, FilterOperation,
        FilterQueryBuilder, Germination, LimitSpec, Rank, Taxon,
    },
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::debug;

use crate::{auth::AuthSession, error, state::AppState, CustomKey};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_taxa))
        .route("/:id", get(show_taxon))
        .route("/:id/samples", get(show_all_children))
        .route("/quickfind", get(quickfind))
        .route("/editgerm", get(editgerm).post(addgerm))
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

async fn show_all_children(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let samples: Vec<Sample> = sqlx::query_as(
        r#"WITH RECURSIVE CTE AS (
        SELECT T.tsn, T.parent_tsn, T.complete_name, T.unit_name1, T.unit_name2, T.unit_name3,
        T.rank_id, T.phylo_sort_seq, T.tsn as top_parent, T.complete_name as top_parent_name
        FROM taxonomic_units T WHERE T.tsn=?
        UNION ALL
        SELECT TT.tsn, TT.parent_tsn, TT.complete_name, TT.unit_name1, TT.unit_name2, TT.unit_name3,
        TT.rank_id, TT.phylo_sort_seq, CTE.top_parent, CTE.top_parent_name
        FROM taxonomic_units TT, CTE
        WHERE TT.parent_tsn = CTE.tsn)
        SELECT CTE.tsn, CTE.parent_tsn as parentid, CTE.complete_name, CTE.unit_name1, CTE.unit_name2, CTE.unit_name3, CTE.phylo_sort_seq as seq, top_parent, top_parent_name,
        S.id, L.locid, L.name as locname, quantity, month, year, notes
        FROM CTE
        INNER JOIN seedsamples S ON CTE.tsn=S.tsn
        INNER JOIN seedlocations L on L.locid=S.collectedlocation
        ORDER BY seq"#)
        .bind(id)
        .fetch_all(&state.dbpool)
        .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                     samples => samples),
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
    let germination = sqlx::query_as!(
        Germination,
        r#"SELECT G.* from germinationcodes G
                                      INNER JOIN taxongermination TG ON TG.germid=G.id
                                      WHERE TG.tsn=?"#,
        id
    )
    .fetch_all(&state.dbpool)
    .await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 taxon => hierarchy[0],
                 parents => hierarchy,
                 children => children,
                 samples => samples,
                 germination => germination,),
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

#[derive(Serialize)]
struct GerminationInfo {
    id: i64,
    code: Option<String>,
    summary: Option<String>,
    description: Option<String>,
}

async fn editgerm(
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let codes = sqlx::query_as!(GerminationInfo, "SELECT * FROM germinationcodes")
        .fetch_all(&state.dbpool)
        .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(codes => codes),
    ))
}

#[derive(Deserialize)]
struct AddGermParams {
    taxon: i64,
    germid: i64,
}

async fn addgerm(
    State(state): State<AppState>,
    Form(params): Form<AddGermParams>,
) -> Result<impl IntoResponse, error::Error> {
    let newid = sqlx::query!(
        "INSERT INTO taxongermination (tsn, germid) VALUES (?, ?)",
        params.taxon,
        params.germid
    )
    .execute(&state.dbpool)
    .await?
    .last_insert_rowid();
    Ok(format!("<div>Inserted row {newid}</div>"))
}

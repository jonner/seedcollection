use axum::{
    extract::{Path, Query, Request, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    empty_string_as_none,
    filter::{Cmp, CompoundFilter, FilterOp},
    sample::{Filter, Sample},
    taxonomy::{self, any_filter, FilterField, Germination, LimitSpec, Rank, Taxon},
};
use minijinja::context;
use serde::Deserialize;
use sqlx::Row;
use strum::IntoEnumIterator;
use tracing::debug;

use crate::{auth::AuthSession, error, state::AppState, TemplateKey};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_taxa))
        .route("/:id", get(show_taxon))
        .route("/:id/samples", get(show_all_children))
        .route("/datalist", get(datalist))
        .route("/search", get(search))
        .route("/editgerm", get(editgerm).post(addgerm))
}

async fn root(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let ranks: Vec<Rank> = Rank::iter().collect();
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user, ranks => ranks),
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
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
    req: Request,
) -> Result<impl IntoResponse, error::Error> {
    let rank = match params.rank {
        Some(r) => r,
        None => Rank::Species,
    };
    let pg = params.page.unwrap_or(1);
    let row = taxonomy::count_query(Some(Box::new(FilterField::Rank(rank.clone()))))
        .build()
        .fetch_one(&state.dbpool)
        .await?;
    let count = row.try_get::<i32, _>("count")?;
    let total_pages = (count + PAGE_SIZE - 1) / PAGE_SIZE;
    let taxa: Vec<Taxon> = Taxon::fetch_all(
        Some(Box::new(FilterField::Rank(rank))),
        Some(LimitSpec(PAGE_SIZE, Some(PAGE_SIZE * (pg - 1)))),
        &state.dbpool,
    )
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
    TemplateKey(key): TemplateKey,
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
        M.native_status, S.id, L.locid, L.name as locname, quantity, month, year, notes, certainty
        FROM CTE
        INNER JOIN sc_samples S ON CTE.tsn=S.tsn
        INNER JOIN sc_locations L on L.locid=S.collectedlocation
        LEFT JOIN mntaxa M on CTE.tsn=M.tsn 
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
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let Some(user) = auth.user else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let mut taxon = Taxon::fetch(id, &state.dbpool).await?;
    let hierarchy = taxon.fetch_hierarchy(&state.dbpool).await?;
    let children = taxon.fetch_children(&state.dbpool).await?;
    let samples = Sample::fetch_all_user(
        user.id,
        Some(Box::new(Filter::Taxon(Cmp::Equal, id))),
        &state.dbpool,
    )
    .await?;
    taxon.fetch_germination_info(&state.dbpool).await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 taxon => taxon,
                 parents => hierarchy,
                 children => children,
                 samples => samples),
    )
    .into_response())
}

#[derive(Deserialize)]
struct DatalistParams {
    taxon: String,
}

async fn datalist(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(DatalistParams { taxon }): Query<DatalistParams>,
) -> Result<impl IntoResponse, error::Error> {
    quickfind(key, &state, taxon, None, None).await
}

#[derive(Deserialize)]
struct SearchParams {
    taxon: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    rank: Option<Rank>,
    minnesota: Option<bool>,
}

async fn search(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(SearchParams {
        taxon,
        rank,
        minnesota,
    }): Query<SearchParams>,
) -> Result<impl IntoResponse, error::Error> {
    quickfind(key, &state, taxon, rank, minnesota).await
}

async fn quickfind(
    key: String,
    state: &AppState,
    taxon: String,
    rank: Option<Rank>,
    minnesota: Option<bool>,
) -> Result<impl IntoResponse, error::Error> {
    let taxa: Vec<Taxon> = match taxon.is_empty() {
        true => Vec::new(),
        false => {
            let parts = taxon.split(' ');
            let mut filter = CompoundFilter::new(FilterOp::And);
            for part in parts {
                filter.add_filter(Box::new(any_filter(part)));
            }
            if let Some(rank) = rank {
                filter.add_filter(Box::new(FilterField::Rank(rank)));
            }
            if Some(true) == minnesota {
                filter.add_filter(Box::new(FilterField::Minnesota(true)));
            }
            /* FIXME: pagination for /search endpoing? */
            Taxon::fetch_all(
                Some(Box::new(filter)),
                Some(LimitSpec(200, None)),
                &state.dbpool,
            )
            .await?
        }
    };
    Ok(RenderHtml(key, state.tmpl.clone(), context!(taxa => taxa)))
}

async fn editgerm(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let codes = sqlx::query_as!(Germination, "SELECT * FROM sc_germination_codes")
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
        "INSERT INTO sc_taxon_germination (tsn, germid) VALUES (?, ?)",
        params.taxon,
        params.germid
    )
    .execute(&state.dbpool)
    .await?
    .last_insert_rowid();
    Ok(format!("<div>Inserted row {newid}</div>"))
}

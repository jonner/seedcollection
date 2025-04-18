use crate::{
    TemplateKey,
    auth::SqliteUser,
    error::Error,
    state::AppState,
    util::{
        Paginator,
        extract::{Form, Query},
    },
};
use axum::{
    Router,
    extract::{OriginalUri, Path, State},
    response::IntoResponse,
    routing::get,
};
use axum_template::RenderHtml;
use libseed::{
    core::{
        database::Database,
        loadable::Loadable,
        query::{
            LimitSpec,
            filter::{Cmp, and},
        },
    },
    empty_string_as_none,
    sample::{self, Sample},
    taxonomy::{self, Germination, Rank, Taxon},
};
use minijinja::context;
use serde::Deserialize;
use strum::IntoEnumIterator;
use tracing::debug;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_taxa))
        .route("/{id}", get(show_taxon))
        .route("/{id}/samples", get(show_all_children))
        .route("/datalist", get(datalist))
        .route("/search", get(search))
        .route("/editgerm", get(editgerm).post(addgerm))
}

async fn root(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let ranks: Vec<Rank> = Rank::iter().collect();
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user, ranks => ranks),
    ))
}

#[derive(Deserialize)]
struct ListParams {
    rank: Option<Rank>,
    page: Option<u32>,
}

async fn list_taxa(
    mut user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
    uri: OriginalUri,
) -> Result<impl IntoResponse, Error> {
    let rank = match params.rank {
        Some(r) => r,
        None => Rank::Species,
    };
    let count = Taxon::count(Some(taxonomy::Filter::Rank(rank.clone()).into()), &state.db).await?;
    let summary = Paginator::new(
        count as u32,
        user.preferences(&state.db).await?.pagesize.into(),
        params.page,
    );
    let taxa: Vec<Taxon> = Taxon::load_all(
        Some(taxonomy::Filter::Rank(rank).into()),
        None,
        Some(summary.limits()),
        &state.db,
    )
    .await?;
    debug!("uri={:?}", uri);
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 taxa => taxa,
                 summary => summary,
                 request_uri => uri.to_string()),
    ))
}

async fn show_all_children(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, Error> {
    let samples: Vec<Sample> = sqlx::query_as(
        r#"WITH RECURSIVE children(t) AS (
            VALUES(?)
            UNION ALL
            SELECT
                tsn
            FROM taxonomic_units, children
            WHERE parent_tsn = children.t
            )
            SELECT
                V.*,
                M.native_status
            FROM vsamples V
            LEFT JOIN mntaxa M USING(tsn)
            WHERE V.tsn IN children AND V.userid=?
            ORDER BY seq"#,
    )
    .bind(id)
    .bind(user.id)
    .fetch_all(state.db.pool())
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 samples => samples),
    ))
}

async fn show_taxon(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, Error> {
    let mut taxon = Taxon::load(id, &state.db).await.map_err(|e| match e {
        libseed::Error::DatabaseError(sqlx::Error::RowNotFound) => {
            Error::NotFound(format!("Taxon '{id}' was not found in the database"))
        }
        _ => e.into(),
    })?;
    let hierarchy = taxon.fetch_hierarchy(&state.db).await?;
    let children = taxon.fetch_children(&state.db).await?;
    let samples = Sample::load_all(
        Some(
            and()
                .push(sample::Filter::TaxonId(Cmp::Equal, id))
                .push(sample::Filter::UserId(user.id))
                .build(),
        ),
        None,
        None,
        &state.db,
    )
    .await?;
    taxon.load_germination_info(&state.db).await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 taxon => taxon,
                 parents => hierarchy,
                 children => children,
                 samples => samples),
    ))
}

#[derive(Deserialize)]
struct DatalistParams {
    taxon: String,
}

async fn datalist(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(DatalistParams { taxon }): Query<DatalistParams>,
) -> Result<impl IntoResponse, Error> {
    let taxa = filter_taxa(taxon, None, None, &state.db).await?;
    Ok(RenderHtml(key, state.tmpl.clone(), context!(taxa => taxa)))
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
) -> Result<impl IntoResponse, Error> {
    let taxa = filter_taxa(taxon, rank, minnesota, &state.db).await?;
    Ok(RenderHtml(key, state.tmpl.clone(), context!(taxa => taxa)))
}

async fn filter_taxa(
    taxon: String,
    rank: Option<Rank>,
    minnesota: Option<bool>,
    db: &Database,
) -> Result<Vec<Taxon>, Error> {
    match taxon.is_empty() {
        true => Ok(Vec::new()),
        false => {
            let mut filter = and();
            if let Some(quickfilter) = libseed::taxonomy::quickfind(taxon) {
                filter = filter.push(quickfilter);
            }
            if let Some(rank) = rank {
                filter = filter.push(taxonomy::Filter::Rank(rank));
            }
            if Some(true) == minnesota {
                filter = filter.push(taxonomy::Filter::Minnesota(true));
            }
            /* FIXME: pagination for /search endpoing? */
            Taxon::load_all(
                Some(filter.build()),
                None,
                Some(LimitSpec {
                    count: 200,
                    offset: None,
                }),
                db,
            )
            .await
            .map_err(Into::into)
        }
    }
}

async fn editgerm(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let codes = Germination::load_all(&state.db).await?;
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
) -> Result<impl IntoResponse, Error> {
    let newid = sqlx::query!(
        "INSERT INTO sc_taxon_germination (tsn, germid) VALUES (?, ?)",
        params.taxon,
        params.germid
    )
    .execute(state.db.pool())
    .await?
    .last_insert_rowid();
    Ok(format!("<div>Inserted row {newid}</div>"))
}

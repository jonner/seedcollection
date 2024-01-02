use anyhow::anyhow;
use axum::{
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    collection::Collection,
    empty_string_as_none,
    filter::Cmp,
    location::Location,
    sample::{self, Certainty, Filter, Sample},
    taxonomy::Germination,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;

use crate::{app_url, auth::AuthSession, error, state::AppState, CustomKey, Message, MessageType};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(sample_index))
        .route("/list", get(list_samples))
        .route("/new", get(new_sample).post(insert_sample))
        .route("/filter", get(filter_samples))
        .route("/:id", get(show_sample).put(update_sample))
        .route("/:id/edit", get(show_sample))
}

async fn sample_index(
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

#[derive(Deserialize)]
struct FilterParams {
    fragment: String,
}

async fn filter_samples(
    auth: AuthSession,
    Query(FilterParams { fragment }): Query<FilterParams>,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(Some(Box::new(Filter::TaxonNameLike(fragment))));
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 samples => samples),
    ))
}

async fn list_samples(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(None);
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                                            samples => samples),
    ))
}

async fn show_sample(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(Some(Box::new(Filter::Sample(Cmp::Equal, id))));
    let sample: Sample = builder.build_query_as().fetch_one(&state.dbpool).await?;
    let collection = match sample.collection {
        Some(cid) => {
            let c: Collection =
                sqlx::query_as("SELECT id, name, description FROM seedcollections WHERE id=?")
                    .bind(cid)
                    .fetch_one(&state.dbpool)
                    .await?;
            Some(c)
        }
        None => None,
    };
    let locations: Vec<Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations ORDER BY name ASC",
    )
    .fetch_all(&state.dbpool)
    .await?;
    let germination = sqlx::query_as!(
        Germination,
        r#"SELECT G.* from germinationcodes G
                                      INNER JOIN taxongermination TG ON TG.germid=G.id
                                      WHERE TG.tsn=?"#,
        sample.taxon.id
    )
    .fetch_all(&state.dbpool)
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 sample => sample,
                 germination => germination,
                 locations => locations,
                 collection => collection),
    ))
}

async fn new_sample(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let locations: Vec<Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations ORDER BY name ASC",
    )
    .fetch_all(&state.dbpool)
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 locations => locations),
    ))
}

#[derive(Serialize, Deserialize)]
struct SampleParams {
    #[serde(deserialize_with = "empty_string_as_none")]
    taxon: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    location: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    month: Option<u32>,
    #[serde(deserialize_with = "empty_string_as_none")]
    year: Option<u32>,
    #[serde(deserialize_with = "empty_string_as_none")]
    quantity: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    notes: Option<String>,
    uncertain: Option<bool>,
}

fn validate_sample_params(params: &SampleParams) -> Result<(), anyhow::Error> {
    if params.taxon.is_none() {
        return Err(anyhow!("No taxon specified").into());
    }
    if params.location.is_none() {
        return Err(anyhow!("No location specified").into());
    }
    Ok(())
}

async fn do_insert(
    params: &SampleParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    validate_sample_params(params)?;

    let certainty = match params.uncertain {
        Some(true) => Certainty::Uncertain,
        _ => Certainty::Certain,
    };

    sqlx::query("INSERT INTO seedsamples (tsn, collectedlocation, month, year, quantity, notes, certainty) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind(params.taxon)
        .bind(params.location)
        .bind(params.month)
        .bind(params.year)
        .bind(params.quantity)
        .bind(&params.notes)
        .bind(certainty)
        .execute(&state.dbpool)
        .await.map_err(|e| e.into())
}

async fn insert_sample(
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let locations: Vec<Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations ORDER BY name ASC",
    )
    .fetch_all(&state.dbpool)
    .await?;

    let (request, message) = match do_insert(&params, &state).await {
        Err(e) => (
            Some(&params),
            Message {
                r#type: MessageType::Error,
                msg: format!("Failed to save sample: {}", e.0.to_string()),
            },
        ),
        Ok(result) => {
            let id = result.last_insert_rowid();
            let sample: Sample =
                sample::build_query(Some(Box::new(Filter::Sample(Cmp::Equal, id))))
                    .build_query_as()
                    .fetch_one(&state.dbpool)
                    .await?;

            let sampleurl = app_url(&format!("/sample/{}", sample.id));
            (
                None,
                Message {
                    r#type: MessageType::Success,
                    msg: format!(
                        "Added new sample <a href=\"{}\">{}: {}</a> to the database",
                        sampleurl, sample.id, sample.taxon.complete_name
                    ),
                },
            )
        }
    };
    Ok(RenderHtml(
        key + ".partial",
        state.tmpl.clone(),
        context!(locations => locations,
                     message => message,
                     request => request),
    ))
}

async fn do_update(
    id: i64,
    params: &SampleParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    validate_sample_params(params)?;

    let certainty = match params.uncertain {
        Some(true) => Certainty::Uncertain,
        _ => Certainty::Certain,
    };

    sqlx::query("Update seedsamples SET tsn=?, collectedlocation=?, month=?, year=?, quantity=?, notes=?, certainty=? WHERE id=?")
        .bind(params.taxon)
        .bind(params.location)
        .bind(params.month)
        .bind(params.year)
        .bind(params.quantity)
        .bind(&params.notes)
        .bind(certainty)
        .bind(id)
        .execute(&state.dbpool)
        .await.map_err(|e| e.into())
}

async fn update_sample(
    Path(id): Path<i64>,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let locations: Vec<Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations ORDER BY name ASC",
    )
    .fetch_all(&state.dbpool)
    .await?;

    let (request, message) = match do_update(id, &params, &state).await {
        Err(e) => (
            Some(params),
            Message {
                r#type: MessageType::Error,
                msg: format!("Failed to save sample: {}", e.0.to_string()),
            },
        ),
        Ok(_) => (
            None,
            Message {
                r#type: MessageType::Success,
                msg: format!("Updated sample {}", id),
            },
        ),
    };

    let sample: Sample = sample::build_query(Some(Box::new(Filter::Sample(Cmp::Equal, id))))
        .build_query_as()
        .fetch_one(&state.dbpool)
        .await?;

    Ok(RenderHtml(
        key + ".partial",
        state.tmpl.clone(),
        context!(locations => locations,
                     sample => sample,
                     message => message,
                     request => request),
    ))
}

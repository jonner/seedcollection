use anyhow::anyhow;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    collection::Collection,
    empty_string_as_none,
    location::Location,
    sample::{self, Filter, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;

use crate::{
    app_url, auth::AuthSession, error, state::SharedState, CustomKey, Message, MessageType,
};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/", get(sample_index))
        .route("/list", get(list_samples))
        .route("/:id", get(show_sample).put(update_sample))
        .route("/new", get(new_sample).post(insert_sample))
}

async fn sample_index(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl, context!(user => auth.user)))
}

async fn list_samples(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(None);
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(user => auth.user,
                                            samples => samples),
    ))
}

async fn show_sample(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(Some(Filter::Sample(id)));
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
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(user => auth.user,
                 sample => sample,
                 locations => locations,
                 collection => collection),
    ))
}

async fn new_sample(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let locations: Vec<Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations ORDER BY name ASC",
    )
    .fetch_all(&state.dbpool)
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
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
    state: &SharedState,
) -> Result<SqliteQueryResult, error::Error> {
    validate_sample_params(params)?;

    sqlx::query("INSERT INTO seedsamples (tsn, collectedlocation, month, year, quantity, notes) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(params.taxon)
        .bind(params.location)
        .bind(params.month)
        .bind(params.year)
        .bind(params.quantity)
        .bind(&params.notes)
        .execute(&state.dbpool)
        .await.map_err(|e| e.into())
}

async fn insert_sample(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let locations: Vec<Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations ORDER BY name ASC",
    )
    .fetch_all(&state.dbpool)
    .await?;

    match do_insert(&params, &state).await {
        Err(e) => Ok(RenderHtml(
            key + ".partial",
            state.tmpl,
            context!(locations => locations,
                     message => Message {
                         r#type: MessageType::Error,
                         msg: format!("Failed to save sample: {}", e.0.to_string())
                     },
                     request => params),
        )),
        Ok(result) => {
            let id = result.last_insert_rowid();
            let sample: Sample = sample::build_query(Some(Filter::Sample(id)))
                .build_query_as()
                .fetch_one(&state.dbpool)
                .await?;

            let sampleurl = app_url(&format!("/sample/{}", sample.id));
            Ok(RenderHtml(
                key + ".partial",
                state.tmpl,
                context!(locations => locations,
                message => Message {
                    r#type: MessageType::Success,
                    msg: format!("Added new sample <a href=\"{}\">{}: {}</a> to the database",
                                 sampleurl,
                                 sample.id, sample.taxon.complete_name)
                }),
            ))
        }
    }
}

async fn do_update(
    id: i64,
    params: &SampleParams,
    state: &SharedState,
) -> Result<SqliteQueryResult, error::Error> {
    validate_sample_params(params)?;

    sqlx::query("Update seedsamples SET tsn=?, collectedlocation=?, month=?, year=?, quantity=?, notes=? WHERE id=?")
        .bind(params.taxon)
        .bind(params.location)
        .bind(params.month)
        .bind(params.year)
        .bind(params.quantity)
        .bind(&params.notes)
        .bind(id)
        .execute(&state.dbpool)
        .await.map_err(|e| e.into())
}

async fn update_sample(
    Path(id): Path<i64>,
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let locations: Vec<Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations ORDER BY name ASC",
    )
    .fetch_all(&state.dbpool)
    .await?;

    match do_update(id, &params, &state).await {
        Err(e) => {
            let sample: Sample = sample::build_query(Some(Filter::Sample(id)))
                .build_query_as()
                .fetch_one(&state.dbpool)
                .await?;

            Ok(RenderHtml(
                key + ".partial",
                state.tmpl,
                context!(locations => locations,
                     sample => sample,
                     message => Message {
                         r#type: MessageType::Error,
                         msg: format!("Failed to save sample: {}", e.0.to_string())
                     },
                     request => params),
            ))
        }
        Ok(_) => {
            let sample: Sample = sample::build_query(Some(Filter::Sample(id)))
                .build_query_as()
                .fetch_one(&state.dbpool)
                .await?;

            Ok(RenderHtml(
                key + ".partial",
                state.tmpl,
                context!(locations => locations,
                         sample => sample,
                         message => Message {
                    r#type: MessageType::Success,
                    msg: format!("Updated sample {}", sample.id)
                }),
            ))
        }
    }
}

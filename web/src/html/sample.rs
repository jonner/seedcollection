use anyhow::anyhow;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
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
use sqlx::{QueryBuilder, Sqlite};

use crate::{error, state::SharedState, CustomKey};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/list", get(list_samples))
        .route("/:id", get(show_sample).put(update_sample))
        .route("/new", get(new_sample).post(insert_sample))
}

async fn list_samples(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(None);
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(RenderHtml(key, state.tmpl, context!(samples => samples)))
}

async fn show_sample(
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
        context!(sample => sample, locations => locations, collection => collection),
    ))
}

async fn new_sample(
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
        context!(locations => locations),
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

async fn do_insert(params: &SampleParams, state: &SharedState) -> Result<(), error::Error> {
    if params.taxon.is_none() {
        return Err(anyhow!("No taxon specified").into());
    }
    if params.location.is_none() {
        return Err(anyhow!("No location specified").into());
    }

    sqlx::query("INSERT INTO seedsamples (tsn, collectedlocation, month, year, quantity, notes) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(params.taxon)
        .bind(params.location)
        .bind(params.month)
        .bind(params.year)
        .bind(params.quantity)
        .bind(&params.notes)
        .execute(&state.dbpool)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(())
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

    let res = do_insert(&params, &state).await;
    Ok(RenderHtml(
        key + ".partial",
        state.tmpl,
        context!(locations => locations, error => res.err().map(|error::Error(e)| e.to_string()), request => params),
    ))
}

async fn update_sample() -> () {
    todo!();
}

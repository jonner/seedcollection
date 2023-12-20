use std::str::FromStr;

use crate::{error, state::SharedState};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, State},
    response::{Html, Json},
    routing::{get, post},
    Form, Router,
};
use libseed::sample::{self, Filter, Sample};
use serde::{Deserialize, Deserializer};
use sqlx::QueryBuilder;
use sqlx::Sqlite;

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_samples))
        .route(
            "/:id",
            get(show_sample).put(modify_sample).delete(delete_sample),
        )
        .route("/new", post(new_sample))
}

async fn root(State(_state): State<SharedState>) -> Html<String> {
    Html("Samples".to_string())
}

async fn list_samples(State(state): State<SharedState>) -> Result<Json<Vec<Sample>>, error::Error> {
    let mut builder = sample::build_query(None);
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(Json(samples))
}

async fn show_sample(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Result<Json<Sample>, error::Error> {
    let mut builder = sample::build_query(Some(Filter::Sample(id)));
    let sample: Sample = builder.build_query_as().fetch_one(&state.dbpool).await?;
    Ok(Json(sample))
}

fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s)
            .map_err(serde::de::Error::custom)
            .map(Some),
    }
}

#[derive(Deserialize)]
struct SampleParams {
    taxon: Option<i64>,
    location: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    month: Option<u32>,
    #[serde(deserialize_with = "empty_string_as_none")]
    year: Option<u32>,
    #[serde(deserialize_with = "empty_string_as_none")]
    quantity: Option<i64>,
    notes: Option<String>,
}

async fn modify_sample(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Form(params): Form<SampleParams>,
) -> Result<(), error::Error> {
    if params.taxon.is_none()
        && params.location.is_none()
        && params.quantity.is_none()
        && params.month.is_none()
        && params.year.is_none()
        && params.notes.is_none()
    {
        return Err(anyhow!("No params specified").into());
    }
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE seedsamples SET ");
    let mut sep = builder.separated(", ");
    if let Some(id) = params.taxon {
        sep.push_bind("tsn=");
        sep.push_bind_unseparated(id);
    }
    if let Some(id) = params.location {
        sep.push_bind("collectedlocation=");
        sep.push_bind_unseparated(id);
    }
    if let Some(m) = params.month {
        sep.push_bind("month=");
        sep.push_bind_unseparated(m);
    }
    if let Some(y) = params.year {
        sep.push_bind("year=");
        sep.push_bind_unseparated(y);
    }
    if let Some(notes) = params.notes {
        sep.push_bind("notes=");
        sep.push_bind_unseparated(notes);
    }
    builder.push(" WHERE id=");
    builder.push_bind(id);
    builder.build().execute(&state.dbpool).await?;
    Ok(())
}

async fn new_sample(
    State(state): State<SharedState>,
    Form(params): Form<SampleParams>,
) -> Result<(), error::Error> {
    if params.taxon.is_none() && params.location.is_none() {
        return Err(anyhow!("Taxon and Location are required").into());
    }
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
        "INSERT INTO seedsamples (tsn, collectedlocation, month, year, notes) values (",
    );
    let mut sep = builder.separated(", ");
    sep.push_bind(params.taxon);
    sep.push_bind(params.location);
    sep.push_bind(params.month);
    sep.push_bind(params.year);
    sep.push_bind(params.notes);
    builder.push(")");
    builder.build().execute(&state.dbpool).await?;
    Ok(())
}

async fn delete_sample(
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Result<(), error::Error> {
    sqlx::query("DELETE FROM seedsamples WHERE id=?")
        .bind(id)
        .execute(&state.dbpool)
        .await?;
    Ok(())
}

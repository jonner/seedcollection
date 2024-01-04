use crate::{error, state::AppState};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, State},
    response::{Html, Json},
    routing::{get, post},
    Form, Router,
};
use libseed::{empty_string_as_none, sample::Sample};
use serde::Deserialize;
use sqlx::QueryBuilder;
use sqlx::Sqlite;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_samples))
        .route(
            "/:id",
            get(show_sample).put(modify_sample).delete(delete_sample),
        )
        .route("/new", post(new_sample))
}

async fn root(State(_state): State<AppState>) -> Html<String> {
    Html("Samples".to_string())
}

async fn list_samples(State(state): State<AppState>) -> Result<Json<Vec<Sample>>, error::Error> {
    let samples = Sample::fetch_all(None, &state.dbpool).await?;
    Ok(Json(samples))
}

async fn show_sample(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Sample>, error::Error> {
    let sample = Sample::fetch(id, &state.dbpool).await?;
    Ok(Json(sample))
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
    #[serde(deserialize_with = "empty_string_as_none")]
    notes: Option<String>,
}

async fn modify_sample(
    State(state): State<AppState>,
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
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE sc_samples SET ");
    let mut sep = builder.separated(", ");
    if let Some(t) = params.taxon {
        sep.push(" tsn=");
        sep.push_bind_unseparated(t);
    }
    if let Some(l) = params.location {
        sep.push(" collectedlocation=");
        sep.push_bind_unseparated(l);
    }
    if let Some(m) = params.month {
        sep.push(" month=");
        sep.push_bind_unseparated(m);
    }
    if let Some(y) = params.year {
        sep.push(" year=");
        sep.push_bind_unseparated(y);
    }
    if let Some(n) = params.quantity {
        sep.push(" quantity=");
        sep.push_bind_unseparated(n);
    }
    if let Some(notes) = params.notes {
        sep.push(" notes=");
        sep.push_bind_unseparated(notes);
    }
    builder.push(" WHERE id=");
    builder.push_bind(id);
    builder.build().execute(&state.dbpool).await?;
    Ok(())
}

async fn new_sample(
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<(), error::Error> {
    if params.taxon.is_none() && params.location.is_none() {
        return Err(anyhow!("Taxon and Location are required").into());
    }
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
        "INSERT INTO sc_samples (tsn, collectedlocation, month, year, quantity, notes) values (",
    );
    let mut sep = builder.separated(", ");
    sep.push_bind(params.taxon);
    sep.push_bind(params.location);
    sep.push_bind(params.month);
    sep.push_bind(params.year);
    sep.push_bind(params.quantity);
    sep.push_bind(params.notes);
    builder.push(")");
    builder.build().execute(&state.dbpool).await?;
    Ok(())
}

async fn delete_sample(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(), error::Error> {
    sqlx::query("DELETE FROM sc_samples WHERE id=?")
        .bind(id)
        .execute(&state.dbpool)
        .await?;
    Ok(())
}

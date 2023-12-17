use crate::{error, state::SharedState};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, State},
    response::{Html, Json},
    routing::get,
    Form, Router,
};
use libseed::sample::{self, Sample};
use serde::{Deserialize, Deserializer};
use sqlx::QueryBuilder;
use sqlx::Sqlite;
use std::sync::Arc;

pub fn router() -> Router<Arc<SharedState>> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_samples))
        .route("/:id", get(show_sample).put(modify_sample))
}

async fn root(State(_state): State<Arc<SharedState>>) -> Html<String> {
    Html("Samples".to_string())
}

async fn list_samples(
    State(state): State<Arc<SharedState>>,
) -> Result<Json<Vec<Sample>>, error::Error> {
    let mut builder = sample::build_query(None, None);
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(Json(samples))
}

async fn show_sample(
    State(state): State<Arc<SharedState>>,
    Path(id): Path<i64>,
) -> Result<Json<Sample>, error::Error> {
    let mut builder = sample::build_query(None, Some(id));
    let sample: Sample = builder.build_query_as().fetch_one(&state.dbpool).await?;
    Ok(Json(sample))
}

#[derive(Deserialize)]
struct ModifyParams {
    taxon: Option<i64>,
    location: Option<i64>,
    month: Option<u32>,
    year: Option<u32>,
    quantity: Option<i64>,
    notes: Option<String>,
}

async fn modify_sample(
    State(state): State<Arc<SharedState>>,
    Path(id): Path<i64>,
    Form(params): Form<ModifyParams>,
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
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("UPDATE_seedsamles SET ");
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

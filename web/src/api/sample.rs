use crate::{
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Form, Router,
};
use libseed::{
    empty_string_as_none,
    loadable::Loadable,
    location::Location,
    sample::{Certainty, Sample},
    taxonomy::Taxon,
};
use serde::Deserialize;

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

async fn list_samples(
    user: SqliteUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let samples = Sample::fetch_all_user(user.id, None, &state.dbpool).await?;
    Ok(Json(samples).into_response())
}

async fn show_sample(
    _user: SqliteUser,
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
    certainty: Option<Certainty>,
}

async fn modify_sample(
    _user: SqliteUser,
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
        && params.certainty.is_none()
    {
        return Err(anyhow!("No params specified").into());
    }
    let mut sample = Sample::fetch(id, &state.dbpool).await?;
    if let Some(taxon) = params.taxon {
        sample.taxon = Taxon::new_loadable(taxon);
    }
    if let Some(location) = params.location {
        sample.location = Location::new_loadable(location);
    }
    if let Some(month) = params.month {
        sample.month = Some(month);
    }
    if let Some(year) = params.year {
        sample.year = Some(year);
    }
    if let Some(notes) = params.notes {
        sample.notes = Some(notes);
    }
    if let Some(quantity) = params.quantity {
        sample.quantity = Some(quantity);
    }
    if let Some(certainty) = params.certainty {
        sample.certainty = certainty;
    }
    sample.update(&state.dbpool).await?;
    Ok(())
}

async fn new_sample(
    user: SqliteUser,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<(), error::Error> {
    if params.taxon.is_none() && params.location.is_none() {
        return Err(anyhow!("Taxon and Location are required").into());
    }
    let mut sample = Sample::new(
        params.taxon.ok_or_else(|| anyhow!("No taxon specified"))?,
        user.id,
        params
            .location
            .ok_or_else(|| anyhow!("No location specified"))?,
        params.month,
        params.year,
        params.quantity,
        params.notes,
        params.certainty.unwrap_or(Certainty::Certain),
    );
    sample.insert(&state.dbpool).await?;
    Ok(())
}

async fn delete_sample(
    user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(), error::Error> {
    let sample = Sample::fetch(id, &state.dbpool).await?;
    if sample.user.id != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete sample".to_string(),
        ));
    }
    sample.delete(&state.dbpool).await?;
    Ok(())
}

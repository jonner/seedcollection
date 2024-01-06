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
    filter::FilterPart,
    location::Location,
    sample::{Certainty, Filter, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;

use crate::{
    app_url, auth::AuthSession, error, state::AppState, Message, MessageType, TemplateKey,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(sample_index))
        .route("/list", get(list_samples))
        .route("/new", get(new_sample).post(insert_sample))
        .route("/filter", get(filter_samples))
        .route(
            "/:id",
            get(show_sample).put(update_sample).delete(delete_sample),
        )
        .route("/:id/edit", get(show_sample))
}

async fn sample_index(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
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
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let filter: Option<Box<dyn FilterPart>> = match fragment.is_empty() {
        true => None,
        false => Some(Box::new(Filter::TaxonNameLike(fragment))),
    };
    let samples = Sample::fetch_all(filter, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 samples => samples),
    ))
}

async fn list_samples(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let samples = Sample::fetch_all(None, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                                            samples => samples),
    ))
}

async fn show_sample(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let mut sample = Sample::fetch(id, &state.dbpool).await?;
    let collections: Vec<Collection> = sqlx::query_as(
        r#"SELECT C.id, C.name, C.description FROM sc_collections C INNER JOIN sc_collection_samples CS
        ON C.id == CS.collectionid WHERE CS.sampleid = ?"#)
        .bind(id)
        .fetch_all(&state.dbpool)
        .await?;
    sample.taxon.fetch_germination_info(&state.dbpool).await?;

    // needed for edit form
    let locations = Location::fetch_all(&state.dbpool).await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 sample => sample,
                 locations => locations,
                 collections => collections),
    ))
}

async fn new_sample(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let locations = Location::fetch_all(&state.dbpool).await?;
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
        return Err(anyhow!("No taxon specified"));
    }
    if params.location.is_none() {
        return Err(anyhow!("No location specified"));
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

    sqlx::query("INSERT INTO sc_samples (tsn, collectedlocation, month, year, quantity, notes, certainty) VALUES (?, ?, ?, ?, ?, ?, ?)")
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
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let locations = Location::fetch_all(&state.dbpool).await?;
    let (request, message) = match do_insert(&params, &state).await {
        Err(e) => (
            Some(&params),
            Message {
                r#type: MessageType::Error,
                msg: format!("Failed to save sample: {}", e.0),
            },
        ),
        Ok(result) => {
            let id = result.last_insert_rowid();
            let sample = Sample::fetch(id, &state.dbpool).await?;

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
        key,
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

    sqlx::query("Update sc_samples SET tsn=?, collectedlocation=?, month=?, year=?, quantity=?, notes=?, certainty=? WHERE id=?")
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
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let locations = Location::fetch_all(&state.dbpool).await?;
    let (request, message) = match do_update(id, &params, &state).await {
        Err(e) => (
            Some(params),
            Message {
                r#type: MessageType::Error,
                msg: format!("Failed to save sample: {}", e.0),
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

    let sample = Sample::fetch(id, &state.dbpool).await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(locations => locations,
                     sample => sample,
                     message => message,
                     request => request),
    ))
}

async fn delete_sample(
    Path(id): Path<i64>,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let context = match sqlx::query("DELETE FROM sc_samples WHERE id=?")
        .bind(id)
        .execute(&state.dbpool)
        .await
    {
        Err(e) => {
            let locations = Location::fetch_all(&state.dbpool).await?;
            let sample = Sample::fetch(id, &state.dbpool).await?;
            context!(locations => locations,
            sample => sample,
            message => Message {
                r#type: MessageType::Error,
                msg: format!("Error deleting sample: {}", e),
            })
        }
        Ok(_) => context!(deleted => true,
        message => Message {
            r#type: MessageType::Success,
            msg: format!("Deleted sample {id}"),
        }),
    };

    Ok(RenderHtml(key, state.tmpl.clone(), context))
}

use std::sync::Arc;

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
    filter::{Cmp, FilterBuilder, FilterOp},
    loadable::Loadable,
    location::{self, Location},
    sample::{self, Certainty, Sample},
    taxonomy::Taxon,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;

use crate::{
    app_url,
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
    Message, MessageType, TemplateKey,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/list", get(list_samples))
        .route("/new", get(new_sample).post(insert_sample))
        .route("/filter", get(filter_samples))
        .route(
            "/:id",
            get(show_sample).put(update_sample).delete(delete_sample),
        )
        .route("/:id/edit", get(show_sample))
}

#[derive(Deserialize)]
struct FilterParams {
    fragment: String,
}

async fn filter_samples(
    user: SqliteUser,
    Query(FilterParams { fragment }): Query<FilterParams>,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut fbuilder =
        FilterBuilder::new(FilterOp::And).push(Arc::new(sample::Filter::User(user.id)));
    if !fragment.is_empty() {
        let subfilter = FilterBuilder::new(FilterOp::Or)
            .push(Arc::new(sample::Filter::TaxonNameLike(fragment.clone())))
            .push(Arc::new(sample::Filter::Notes(Cmp::Like, fragment.clone())))
            .push(Arc::new(location::Filter::Name(
                Cmp::Like,
                fragment.clone(),
            )))
            .build();
        fbuilder = fbuilder.push(subfilter);
    };
    let samples = Sample::fetch_all_user(user.id, Some(fbuilder.build()), &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 samples => samples),
    )
    .into_response())
}

async fn list_samples(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match Sample::fetch_all_user(user.id, None, &state.dbpool).await {
        Ok(samples) => RenderHtml(
            key,
            state.tmpl.clone(),
            context!(user => user,
                     samples => samples),
        )
        .into_response(),
        Err(e) => error::Error::from(e).into_response(),
    }
}

async fn show_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let mut sample = Sample::fetch(id, &state.dbpool).await?;
    let collections: Vec<Collection> = sqlx::query_as(
        r#"SELECT C.id, C.name, C.description, C.userid FROM sc_collections C INNER JOIN sc_collection_samples CS
        ON C.id == CS.collectionid WHERE CS.sampleid = ?"#)
        .bind(id)
        .fetch_all(&state.dbpool)
        .await?;
    sample.taxon.fetch_germination_info(&state.dbpool).await?;

    // needed for edit form
    let locations = Location::fetch_all_user(user.id, &state.dbpool).await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 sample => sample,
                 locations => locations,
                 collections => collections),
    )
    .into_response())
}

async fn new_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let locations = Location::fetch_all_user(user.id, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 locations => locations),
    )
    .into_response())
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

async fn do_insert(
    user: &SqliteUser,
    params: &SampleParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    let certainty = match params.uncertain {
        Some(true) => Certainty::Uncertain,
        _ => Certainty::Certain,
    };

    let mut sample = Sample::new(
        params.taxon.ok_or(anyhow!("No taxon provided"))?,
        user.id,
        params.location.ok_or(anyhow!("No location provided"))?,
        params.month,
        params.year,
        params.quantity,
        params.notes.clone(),
        certainty,
    );
    sample.insert(&state.dbpool).await.map_err(|e| e.into())
}

async fn insert_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let locations = Location::fetch_all_user(user.id, &state.dbpool).await?;
    match do_insert(&user, &params, &state).await {
        Err(e) => Ok(RenderHtml(
            key,
            state.tmpl.clone(),
            context!(locations => locations,
                         message => Message {
                             r#type: MessageType::Error,
                             msg: format!("Failed to save sample: {}", e),
                         },
                         request => params),
        )
        .into_response()),
        Ok(result) => {
            let id = result.last_insert_rowid();
            let sample = Sample::fetch(id, &state.dbpool).await?;

            let sampleurl = app_url(&format!("/sample/{}", sample.id));
            Ok((
                [("HX-Redirect", sampleurl)],
                RenderHtml(
                    key,
                    state.tmpl.clone(),
                    context!(locations => locations,
                    message => Message {
                        r#type: MessageType::Success,
                        msg: format!(
                            "Added new sample {}: {} to the database",
                            sample.id, sample.taxon.complete_name
                            ),
                    },
                    ),
                ),
            )
                .into_response())
        }
    }
}

async fn do_update(
    id: i64,
    params: &SampleParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    let certainty = match params.uncertain {
        Some(true) => Certainty::Uncertain,
        _ => Certainty::Certain,
    };
    let mut sample = Sample::fetch(id, &state.dbpool).await?;
    sample.taxon = Taxon::new_loadable(params.taxon.ok_or_else(|| anyhow!("No taxon specified"))?);
    sample.location = Location::new_loadable(
        params
            .location
            .ok_or_else(|| anyhow!("No location specified"))?,
    );
    sample.month = params.month;
    sample.year = params.year;
    sample.quantity = params.quantity;
    sample.notes = params.notes.as_ref().cloned();
    sample.certainty = certainty;
    sample.update(&state.dbpool).await.map_err(|e| e.into())
}

async fn update_sample(
    user: SqliteUser,
    Path(id): Path<i64>,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let locations = Location::fetch_all_user(user.id, &state.dbpool).await?;
    let (request, message, headers) = match do_update(id, &params, &state).await {
        Err(e) => (
            Some(params),
            Message {
                r#type: MessageType::Error,
                msg: format!("Failed to save sample: {}", e),
            },
            None,
        ),
        Ok(_) => (
            None,
            Message {
                r#type: MessageType::Success,
                msg: format!("Updated sample {}", id),
            },
            Some([("HX-Redirect", app_url(&format!("/sample/{id}")))]),
        ),
    };

    let sample = Sample::fetch(id, &state.dbpool).await?;

    Ok((
        headers,
        RenderHtml(
            key,
            state.tmpl.clone(),
            context!(locations => locations,
                     sample => sample,
                     message => message,
                     request => request),
        ),
    )
        .into_response())
}

async fn delete_sample(
    user: SqliteUser,
    Path(id): Path<i64>,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let sample = Sample::fetch(id, &state.dbpool).await?;
    if sample.user.id != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete sample".to_string(),
        ));
    }
    match sample.delete(&state.dbpool).await {
        Err(e) => {
            let locations = Location::fetch_all_user(user.id, &state.dbpool).await?;
            let sample = Sample::fetch(id, &state.dbpool).await?;
            Ok(RenderHtml(
                key,
                state.tmpl.clone(),
                context!(locations => locations,
                sample => sample,
                message => Message {
                    r#type: MessageType::Error,
                    msg: format!("Error deleting sample: {}", e),
                }),
            )
            .into_response())
        }
        Ok(_) => Ok((
            [("HX-Redirect", app_url("/sample/list"))],
            RenderHtml(
                key,
                state.tmpl.clone(),
                context!(deleted => true,
                message => Message {
                    r#type: MessageType::Success,
                    msg: format!("Deleted sample {id}"),
                }),
            ),
        )
            .into_response()),
    }
}

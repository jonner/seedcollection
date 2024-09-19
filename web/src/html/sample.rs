use crate::{
    app_url,
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
    Message, MessageType, TemplateKey,
};
use anyhow::anyhow;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    empty_string_as_none,
    filter::{Cmp, CompoundFilter, Op, SortOrder, SortSpec},
    loadable::{ExternalRef, Loadable},
    project::{allocation, Allocation},
    sample::{self, Certainty, Sample, SortField, SortSpecs},
    source::Source,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;
use std::sync::Arc;
use tracing::debug;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/list", get(list_samples))
        .route("/new", get(new_sample).post(insert_sample))
        .route(
            "/:id",
            get(show_sample).put(update_sample).delete(delete_sample),
        )
        .route("/:id/edit", get(show_sample))
}

#[derive(Debug, Deserialize)]
struct SampleListParams {
    #[serde(deserialize_with = "empty_string_as_none")]
    filter: Option<String>,
    sort: Option<SortField>,
    order: Option<SortOrder>,
}

async fn list_samples(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    query: Option<Query<SampleListParams>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    debug!("query params: {:?}", query);
    let mut filter = None;
    let mut sort = None;
    match query {
        Some(params) => {
            filter = params.filter.as_ref().map(|f| {
                CompoundFilter::builder(Op::Or)
                    .push(sample::Filter::TaxonNameLike(f.clone()))
                    .push(sample::Filter::Notes(Cmp::Like, f.clone()))
                    .push(sample::Filter::SourceNameLike(f.clone()))
                    .build()
            });
            let o = params
                .order
                .as_ref()
                .cloned()
                .unwrap_or(SortOrder::Ascending);
            let s = params
                .sort
                .as_ref()
                .cloned()
                .unwrap_or(SortField::TaxonSequence);
            sort = Some(SortSpecs(vec![SortSpec::new(s, o)]));
        }
        None => (),
    };
    match Sample::load_all_user(user.id, filter, sort, &state.dbpool).await {
        Ok(samples) => RenderHtml(
            key,
            state.tmpl.clone(),
            context!(user => user,
                     samples => samples,
                     filteronly => headers.get("HX-Request").is_some()),
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
    let mut sample = Sample::load(id, &state.dbpool).await?;
    sample
        .taxon
        .object_mut()?
        .load_germination_info(&state.dbpool)
        .await?;

    // needed for edit form
    let sources = Source::load_all_user(user.id, &state.dbpool).await?;

    let mut allocations = Allocation::load_all(
        Some(Arc::new(allocation::Filter::SampleId(id))),
        None,
        &state.dbpool,
    )
    .await?;
    for alloc in allocations.iter_mut() {
        alloc.load_notes(&state.dbpool).await?;
    }

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 sample => sample,
                 sources => sources,
                 allocations => allocations),
    )
    .into_response())
}

async fn new_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let sources = Source::load_all_user(user.id, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 sources => sources),
    )
    .into_response())
}

#[derive(Serialize, Deserialize)]
struct SampleParams {
    #[serde(deserialize_with = "empty_string_as_none")]
    taxon: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    source: Option<i64>,
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
        params.source.ok_or(anyhow!("No source provided"))?,
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
    let sources = Source::load_all_user(user.id, &state.dbpool).await?;
    match do_insert(&user, &params, &state).await {
        Err(e) => Ok(RenderHtml(
            key,
            state.tmpl.clone(),
            context!(sources => sources,
                         message => Message {
                             r#type: MessageType::Error,
                             msg: format!("Failed to save sample: {}", e),
                         },
                         request => params),
        )
        .into_response()),
        Ok(result) => {
            let id = result.last_insert_rowid();
            let sample = Sample::load(id, &state.dbpool).await?;

            let sampleurl = app_url(&format!("/sample/{}", sample.id));
            Ok((
                [("HX-Redirect", sampleurl)],
                RenderHtml(
                    key,
                    state.tmpl.clone(),
                    context!(sources => sources,
                    message => Message {
                        r#type: MessageType::Success,
                        msg: format!(
                            "Added new sample {}: {} to the database",
                            sample.id, sample.taxon.object()?.complete_name
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
    let mut sample = Sample::load(id, &state.dbpool).await?;
    sample.taxon = ExternalRef::Stub(params.taxon.ok_or_else(|| anyhow!("No taxon specified"))?);
    sample.source = ExternalRef::Stub(
        params
            .source
            .ok_or_else(|| anyhow!("No source specified"))?,
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
    let sources = Source::load_all_user(user.id, &state.dbpool).await?;
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

    let sample = Sample::load(id, &state.dbpool).await?;

    Ok((
        headers,
        RenderHtml(
            key,
            state.tmpl.clone(),
            context!(sources => sources,
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
    let mut sample = Sample::load(id, &state.dbpool).await?;
    if sample.user.id() != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete sample".to_string(),
        ));
    }
    match sample.delete(&state.dbpool).await {
        Err(e) => {
            let sources = Source::load_all_user(user.id, &state.dbpool).await?;
            let sample = Sample::load(id, &state.dbpool).await?;
            Ok(RenderHtml(
                key,
                state.tmpl.clone(),
                context!(sources => sources,
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

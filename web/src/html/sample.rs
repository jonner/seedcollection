use crate::{
    Message, MessageType, TemplateKey,
    auth::SqliteUser,
    error::{self, Error},
    html::SortOption,
    state::AppState,
    util::app_url,
};
use anyhow::anyhow;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::get,
};
use axum_template::RenderHtml;
use libseed::{
    core::{
        loadable::{ExternalRef, Loadable},
        query::{Cmp, CompoundFilter, Op, SortOrder, SortSpec, SortSpecs},
    },
    empty_string_as_none,
    project::{Allocation, allocation},
    sample::{self, Certainty, Sample, SortField},
    source::Source,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;
use std::sync::Arc;
use tracing::debug;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/list", get(list_samples))
        .route("/new", get(new_sample).post(insert_sample))
        .route(
            "/{id}",
            get(show_sample).put(update_sample).delete(delete_sample),
        )
        .route("/{id}/edit", get(show_sample))
}

#[derive(Debug, Deserialize, Serialize)]
struct SampleListParams {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    filter: Option<String>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    sort: Option<SortField>,
    #[serde(default, deserialize_with = "empty_string_as_none")]
    dir: Option<SortOrder>,
}

async fn list_samples(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(params): Query<SampleListParams>,
    headers: HeaderMap,
) -> impl IntoResponse {
    debug!("query params: {:?}", params);

    let filter = params.filter.as_ref().map(|f| {
        let idprefix: Result<i64, _> = f.parse();
        let mut builder = CompoundFilter::builder(Op::Or)
            .push(sample::taxon_name_like(f))
            .push(sample::Filter::Notes(Cmp::Like, f.clone()))
            .push(sample::Filter::SourceName(Cmp::Like, f.clone()));
        if let Ok(n) = idprefix {
            builder = builder.push(sample::Filter::Id(Cmp::NumericPrefix, n));
        }
        builder.build()
    });

    let dir = params.dir.as_ref().cloned().unwrap_or(SortOrder::Ascending);
    let field = params
        .sort
        .as_ref()
        .cloned()
        .unwrap_or(SortField::TaxonSequence);
    let sort = Some(SortSpecs(vec![SortSpec::new(field.clone(), dir)]));

    let sort_options = vec![
        SortOption {
            code: SortField::TaxonSequence,
            name: "Taxonomic Order".into(),
            selected: matches!(field, SortField::TaxonSequence),
        },
        SortOption {
            code: SortField::TaxonName,
            name: "Taxon name".into(),
            selected: matches!(field, SortField::TaxonName),
        },
        SortOption {
            code: SortField::Id,
            name: "Sample Id".into(),
            selected: matches!(field, SortField::Id),
        },
        SortOption {
            code: SortField::SourceName,
            name: "Seed Source".into(),
            selected: matches!(field, SortField::SourceName),
        },
        SortOption {
            code: SortField::CollectionDate,
            name: "Date Collected".into(),
            selected: matches!(field, SortField::CollectionDate),
        },
        SortOption {
            code: SortField::Quantity,
            name: "Quantity".into(),
            selected: matches!(field, SortField::Quantity),
        },
    ];
    match Sample::load_all_user(user.id, filter, sort, &state.db).await {
        Ok(samples) => RenderHtml(
            key,
            state.tmpl.clone(),
            context!(user => user,
                     samples => samples,
                     query => params,
                     options =>  sort_options,
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
    let mut sample = Sample::load(id, &state.db).await.map_err(|e| match e {
        libseed::Error::DatabaseError(sqlx::Error::RowNotFound) => {
            Error::NotFound(format!("Sample {id} could not be found"))
        }
        _ => e.into(),
    })?;
    sample
        .taxon
        .object_mut()?
        .load_germination_info(&state.db)
        .await?;
    // make sure the source is fully loaded
    sample.source.load(&state.db, true).await?;

    // needed for edit form
    let sources = Source::load_all_user(user.id, None, &state.db).await?;

    let mut allocations = Allocation::load_all(
        Some(Arc::new(allocation::Filter::SampleId(id))),
        None,
        &state.db,
    )
    .await?;
    for alloc in allocations.iter_mut() {
        alloc.load_notes(&state.db).await?;
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
    let sources = Source::load_all_user(user.id, None, &state.db).await?;
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
    month: Option<u8>,
    #[serde(deserialize_with = "empty_string_as_none")]
    year: Option<u32>,
    #[serde(deserialize_with = "empty_string_as_none")]
    quantity: Option<f64>,
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
    sample.insert(&state.db).await.map_err(|e| e.into())
}

async fn insert_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let sources = Source::load_all_user(user.id, None, &state.db).await?;
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
            let sample = Sample::load(id, &state.db).await?;

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
    let mut sample = Sample::load(id, &state.db).await?;
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
    sample.update(&state.db).await.map_err(|e| e.into())
}

async fn update_sample(
    user: SqliteUser,
    Path(id): Path<i64>,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, error::Error> {
    let sources = Source::load_all_user(user.id, None, &state.db).await?;
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

    let sample = Sample::load(id, &state.db).await?;

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
    let mut sample = Sample::load(id, &state.db).await?;
    if sample.user.id() != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete sample".to_string(),
        ));
    }
    match sample.delete(&state.db).await {
        Err(e) => {
            let sources = Source::load_all_user(user.id, None, &state.db).await?;
            let sample = Sample::load(id, &state.db).await?;
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

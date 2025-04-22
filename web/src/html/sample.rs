use crate::{
    TemplateKey,
    auth::SqliteUser,
    error::Error,
    html::FilterSortOption,
    state::AppState,
    util::{
        AccessControlled, FlashMessage, Paginator, app_url,
        extract::{Form, Query},
    },
};
use axum::{
    Router,
    extract::{OriginalUri, Path, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::get,
};
use axum_template::RenderHtml;
use futures::future::try_join_all;
use libseed::{
    core::{
        loadable::{ExternalRef, Loadable},
        query::{
            SortOrder, SortSpec, SortSpecs,
            filter::{Cmp, and, or},
        },
    },
    empty_string_as_none,
    project::{AllocatedSample, Project, allocation},
    sample::{self, Certainty, Sample, SortField},
    source::Source,
};
use minijinja::context;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};
use serde_with::{DisplayFromStr, serde_as};
use std::str::FromStr;
use strum::IntoEnumIterator;
use time::Month;
use tracing::debug;

use super::{FilterSortSpec, flash_message, sort_dirs};

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
    #[serde(flatten)]
    filter: SampleFilterParams,
    #[serde(default)]
    page: Option<u32>,
}

#[serde_as]
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct SampleFilterParams {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub(crate) filter: Option<String>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub(crate) sort: Option<SortField>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub(crate) dir: Option<SortOrder>,

    #[serde(default)]
    // workaround for https://github.com/serde-rs/serde/issues/1183
    // the field is deserialize correctly when it's deserialized directly, but
    // not when it's a flattened struct within another struct.
    #[serde_as(as = "DisplayFromStr")]
    pub(crate) all: bool,
}

async fn list_samples(
    mut user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(mut params): Query<SampleListParams>,
    headers: HeaderMap,
    uri: OriginalUri,
) -> Result<impl IntoResponse, Error> {
    debug!("query params: {:?}", params);

    let user_filter = params.filter.filter.as_ref().map(|f| {
        let idprefix: Result<i64, _> = f.parse();
        let mut builder = or()
            .push(sample::taxon_name_like(f))
            .push(sample::Filter::Notes(Cmp::Like, f.clone()))
            .push(sample::Filter::SourceName(Cmp::Like, f.clone()));
        if let Ok(n) = idprefix {
            builder = builder.push(sample::Filter::Id(Cmp::NumericPrefix, n));
        }
        builder.build()
    });
    let mut builder = and().push(sample::Filter::UserId(user.id));
    if !params.filter.all {
        builder = builder.push(sample::Filter::Quantity(Cmp::NotEqual, 0.0))
    }
    if let Some(filter) = user_filter {
        builder = builder.push(filter);
    }
    let filter = builder.build();

    let dir = params.filter.dir.get_or_insert_default();
    let field = params.filter.sort.get_or_insert(SortField::TaxonSequence);
    let sort = Some(SortSpecs(vec![SortSpec::new(field.clone(), *dir)]));
    let nsamples = Sample::count(Some(filter.clone()), &state.db).await?;
    let summary = Paginator::new(
        nsamples as u32,
        user.preferences(&state.db).await?.pagesize.into(),
        params.page,
    );
    let samples = Sample::load_all(Some(filter), sort, Some(summary.limits()), &state.db).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                     samples => samples,
                     filter_spec => sample_filter_spec(&params.filter),
                     summary => summary,
                     request_uri => uri.to_string(),
                     filteronly => headers.get("HX-Request").is_some()),
    ))
}

pub(crate) fn sample_filter_spec(params: &SampleFilterParams) -> FilterSortSpec<SortField> {
    let sort_fields = SortField::iter()
        .map(|field| FilterSortOption {
            value: field.clone(),
            name: field.to_string(),
            selected: Some(field) == params.sort,
        })
        .collect();
    let sort_dirs = sort_dirs(params.dir.unwrap_or_default());
    let additional_filters = vec![FilterSortOption {
        name: "Show empty samples".into(),
        value: "all".into(),
        selected: params.all,
    }];
    FilterSortSpec {
        filter: params.filter.as_ref().cloned().unwrap_or_default(),
        sort_fields,
        sort_dirs,
        additional_filters,
    }
}

async fn show_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, Error> {
    let mut sample = Sample::load_for_user(id, &user, &state.db).await?;
    sample
        .taxon
        .object_mut()?
        .load_germination_info(&state.db)
        .await?;
    // make sure the source is fully loaded
    sample.source.load(&state.db, true).await?;

    // needed for edit form
    let sources = Source::load_all_user(user.id, None, &state.db).await?;

    let allocations = AllocatedSample::load_all(
        Some(allocation::Filter::SampleId(id).into()),
        None,
        None,
        &state.db,
    )
    .await?;

    let projects = try_join_all(allocations.iter().map(async |alloc| -> libseed::Result<_> {
        Ok((Project::load(alloc.projectid, &state.db).await?, alloc))
    }))
    .await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 sample => sample,
                 sources => sources,
                 projects => projects),
    ))
}

async fn new_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let sources = Source::load_all_user(user.id, None, &state.db).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 sources => sources),
    ))
}

// A utility function to deserialize an Optional month from either a month name
// or a month number. Also treats an empty string as None
pub fn deserialize_month<'de, D>(de: D) -> Result<Option<Month>, D::Error>
where
    D: Deserializer<'de>,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => Month::from_str(s)
            .map_err(<D as Deserializer>::Error::custom)
            .or_else(|_| {
                u8::from_str(s)
                    .map_err(<D as Deserializer>::Error::custom)
                    .and_then(|n| Month::try_from(n).map_err(<D as Deserializer>::Error::custom))
            })
            .map(Some),
    }
}

#[derive(Serialize, Deserialize)]
struct SampleParams {
    #[serde(deserialize_with = "empty_string_as_none")]
    taxon: Option<i64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    source: Option<i64>,
    #[serde(deserialize_with = "deserialize_month")]
    month: Option<Month>,
    #[serde(deserialize_with = "empty_string_as_none")]
    year: Option<u32>,
    #[serde(deserialize_with = "empty_string_as_none")]
    quantity: Option<f64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    notes: Option<String>,
    uncertain: Option<bool>,
}

async fn insert_sample(
    user: SqliteUser,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, Error> {
    let certainty = match params.uncertain {
        Some(true) => Certainty::Uncertain,
        _ => Certainty::Certain,
    };
    let mut sample = Sample::new(
        params
            .taxon
            .ok_or(Error::RequiredParameterMissing("taxon".into()))?,
        user.id,
        params
            .source
            .ok_or(Error::RequiredParameterMissing("source".into()))?,
        params.month,
        params.year,
        params.quantity,
        params.notes.clone(),
        certainty,
    );
    sample.insert(&state.db).await?;

    let sampleurl = app_url(&format!("/sample/{}", sample.id));
    let taxon_name = &sample.taxon.load(&state.db, false).await?.complete_name;
    Ok((
        [("HX-Redirect", sampleurl)],
        flash_message(
            state,
            FlashMessage::Success(format!(
                "Added new sample {}: {} to the database",
                sample.id, taxon_name
            )),
        ),
    )
        .into_response())
}

async fn update_sample(
    user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Form(params): Form<SampleParams>,
) -> Result<impl IntoResponse, Error> {
    let mut sample = Sample::load_for_user(id, &user, &state.db).await?;

    let certainty = match params.uncertain {
        Some(true) => Certainty::Uncertain,
        _ => Certainty::Certain,
    };
    sample.taxon = ExternalRef::Stub(
        params
            .taxon
            .ok_or(Error::RequiredParameterMissing("taxon".into()))?,
    );
    sample.source = ExternalRef::Stub(
        params
            .source
            .ok_or(Error::RequiredParameterMissing("source".into()))?,
    );
    sample.month = params.month;
    sample.year = params.year;
    sample.quantity = params.quantity;
    sample.notes = params.notes.as_ref().cloned();
    sample.certainty = certainty;
    sample.update(&state.db).await?;
    Ok((
        [("HX-Redirect", app_url(&format!("/sample/{id}")))],
        flash_message(
            state,
            FlashMessage::Success(format!("Updated sample {}", id)),
        ),
    ))
}

async fn delete_sample(
    user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let mut sample = Sample::load_for_user(id, &user, &state.db).await?;
    sample.delete(&state.db).await?;
    Ok((
        [("HX-Redirect", app_url("/sample/list"))],
        flash_message(state, FlashMessage::Success(format!("Deleted sample {id}"))),
    ))
}

use crate::{
    TemplateKey,
    auth::SqliteUser,
    error::Error,
    state::AppState,
    util::{
        AccessControlled, FlashMessageKind, Paginator, app_url,
        extract::{Form, Query},
    },
};
use anyhow::{Context, anyhow};
use axum::{
    Router,
    extract::{OriginalUri, Path, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::get,
};
use axum_template::RenderHtml;
use libseed::{
    core::{
        loadable::Loadable,
        query::filter::{Cmp, and, or},
    },
    empty_string_as_none,
    sample::{Filter, Sample},
    source::{self, Source},
};
use minijinja::context;
use serde::{Deserialize, Serialize};

use super::flash_message;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/new", get(add_source).post(new_source))
        .route("/new/modal", get(add_source))
        .route(
            "/{id}",
            get(show_source).put(update_source).delete(delete_source),
        )
        .route("/{id}/edit", get(show_source))
        .route("/list", get(list_sources))
        .route("/list/options", get(list_sources))
}

#[derive(Deserialize)]
struct SourceListParams {
    filter: Option<String>,
    page: Option<u32>,
}

async fn list_sources(
    mut user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(params): Query<SourceListParams>,
    headers: HeaderMap,
    uri: OriginalUri,
) -> Result<impl IntoResponse, Error> {
    let mut fbuilder = and().push(source::Filter::UserId(user.id));

    if let Some(filterstring) = params.filter {
        let subfilter = or()
            .push(source::Filter::Name(Cmp::Like, filterstring.clone()))
            .push(source::Filter::Description(Cmp::Like, filterstring.clone()))
            .build();
        fbuilder = fbuilder.push(subfilter);
    }
    let filter = fbuilder.build();
    let paginator = Paginator::new(
        Source::count(Some(filter.clone()), &state.db).await? as u32,
        user.preferences(&state.db).await?.pagesize.into(),
        params.page,
    );
    let sources =
        Source::load_all(Some(filter), None, paginator.limits().into(), &state.db).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 sources => sources,
                 summary => paginator,
                 request_uri => uri.to_string(),
                 filteronly => headers.get("HX-Request").is_some()),
    )
    .into_response())
}

async fn add_source(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => user)).into_response())
}

#[derive(Debug, Deserialize)]
struct SourceShowParams {
    page: Option<u32>,
}

async fn show_source(
    mut user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    uri: OriginalUri,
    Query(params): Query<SourceShowParams>,
) -> Result<impl IntoResponse, Error> {
    let src = Source::load_for_user(id, &user, &state.db).await?;
    let sample_filter = and()
        .push(Filter::SourceId(Cmp::Equal, id))
        .push(Filter::UserId(user.id))
        .build();
    let paginator = Paginator::new(
        Sample::count(Some(sample_filter.clone()), &state.db).await? as u32,
        user.preferences(&state.db).await?.pagesize.into(),
        params.page,
    );
    let samples = Sample::load_all(
        Some(sample_filter),
        None,
        Some(paginator.limits()),
        &state.db,
    )
    .await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 source => src,
                 summary => paginator,
                 request_uri => uri.to_string(),
                 samples => samples),
    )
    .into_response())
}

#[derive(Debug, Deserialize, Serialize)]
struct SourceEditParams {
    #[serde(deserialize_with = "empty_string_as_none")]
    name: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    description: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    latitude: Option<f64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    longitude: Option<f64>,
    modal: Option<i64>,
}

async fn do_update(
    src: &mut Source,
    params: &SourceEditParams,
    state: &AppState,
) -> Result<(), Error> {
    src.name = params
        .name
        .as_ref()
        .ok_or_else(|| anyhow!("No name specified"))?
        .to_string();
    src.description = params.description.as_ref().cloned();
    src.latitude = params.latitude;
    src.longitude = params.longitude;

    src.update(&state.db).await?;
    Ok(())
}

async fn update_source(
    user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<SourceEditParams>,
) -> Result<impl IntoResponse, Error> {
    let mut src = Source::load_for_user(id, &user, &state.db).await?;
    do_update(&mut src, &params, &state).await?;
    Ok((
        [("HX-Redirect", app_url(&format!("/source/{id}")))],
        flash_message(
            state,
            FlashMessageKind::Success,
            "Successfully updated source".to_string(),
        ),
    )
        .into_response())
}

async fn do_insert(
    user: &SqliteUser,
    params: &SourceEditParams,
    state: &AppState,
) -> Result<Source, Error> {
    let mut source = Source::new(
        params
            .name
            .as_ref()
            .ok_or(anyhow!("No name was given"))?
            .clone(),
        params.description.as_ref().cloned(),
        params.latitude,
        params.longitude,
        user.id,
    );
    source.insert(&state.db).await?;
    Ok(source)
}

async fn new_source(
    user: SqliteUser,
    State(state): State<AppState>,
    Form(params): Form<SourceEditParams>,
) -> Result<impl IntoResponse, Error> {
    let mut headers = HeaderMap::new();
    let source = do_insert(&user, &params, &state).await?;

    let url = app_url(&format!("/source/{}", source.id));
    if params.modal.is_some() {
        headers.append(
            "HX-Trigger",
            "reload-sources"
                .parse()
                .with_context(|| "Failed to parse header")?,
        );
    } else {
        headers.append(
            "HX-redirect",
            url.parse().with_context(|| "Failed to parse header")?,
        );
    }
    Ok((
        headers,
        flash_message(
            state,
            FlashMessageKind::Success,
            format!("Successfully added source {}", source.id),
        ),
    )
        .into_response())
}

async fn delete_source(
    user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let mut src = Source::load_for_user(id, &user, &state.db).await?;
    src.delete(&state.db)
        .await
        .map(|_| [("HX-redirect", app_url("/source/list"))].into_response())
        .map_err(Into::into)
}

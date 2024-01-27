use crate::{app_url, auth::SqliteUser, Message, MessageType, TemplateKey};
use anyhow::{anyhow, Context};
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
    filter::{Cmp, FilterBuilder, FilterOp},
    source,
};
use libseed::{
    sample::{Filter, Sample},
    source::Source,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;
use std::sync::Arc;

use crate::{error, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/new", get(add_source).post(new_source))
        .route("/new/modal", get(add_source))
        .route(
            "/:id",
            get(show_source).put(update_source).delete(delete_source),
        )
        .route("/:id/edit", get(show_source))
        .route("/list", get(list_sources))
        .route("/list/options", get(list_sources))
}

#[derive(Deserialize)]
struct SourceListParams {
    filter: Option<String>,
}

async fn list_sources(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(params): Query<SourceListParams>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, error::Error> {
    let mut fbuilder =
        FilterBuilder::new(FilterOp::And).push(Arc::new(source::Filter::UserId(user.id)));

    if let Some(filterstring) = params.filter {
        let subfilter = FilterBuilder::new(FilterOp::Or)
            .push(Arc::new(source::Filter::Name(
                Cmp::Like,
                filterstring.clone(),
            )))
            .push(Arc::new(source::Filter::Description(
                Cmp::Like,
                filterstring.clone(),
            )))
            .build();
        fbuilder = fbuilder.push(subfilter);
    }
    let sources = Source::fetch_all(Some(fbuilder.build()), &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 sources => sources,
                 filteronly => headers.get("HX-Request").is_some()),
    )
    .into_response())
}

async fn add_source(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => user)).into_response())
}

async fn show_source(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let src = Source::fetch(id, &state.dbpool).await?;
    let samples = Sample::fetch_all_user(
        user.id,
        Some(Arc::new(Filter::SourceId(Cmp::Equal, id))),
        &state.dbpool,
    )
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 source => src,
                 map_viewer => src.map_viewer_uri(12.0),
                 samples => samples),
    )
    .into_response())
}

#[derive(Debug, Deserialize, Serialize)]
struct SourceParams {
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
    id: i64,
    params: &SourceParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    let mut src = Source::fetch(id, &state.dbpool).await?;
    src.name = params
        .name
        .as_ref()
        .ok_or_else(|| anyhow!("No name specified"))?
        .to_string();
    src.description = params.description.as_ref().cloned();
    src.latitude = params.latitude;
    src.longitude = params.longitude;

    src.update(&state.dbpool).await.map_err(|e| e.into())
}

async fn update_source(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<SourceParams>,
) -> Result<impl IntoResponse, error::Error> {
    let src = Source::fetch(id, &state.dbpool).await?;
    if src.userid != user.id {
        return Err(error::Error::Unauthorized("Not yours".to_string()));
    }
    let (request, message, headers) = match do_update(id, &params, &state).await {
        Err(e) => (
            Some(&params),
            Message {
                r#type: MessageType::Error,
                msg: e.to_string(),
            },
            None,
        ),
        Ok(_) => (
            None,
            Message {
                r#type: MessageType::Success,
                msg: "Successfully updated source".to_string(),
            },
            Some([("HX-Redirect", app_url(&format!("/source/{id}")))]),
        ),
    };
    let samples = Sample::fetch_all_user(
        user.id,
        Some(Arc::new(Filter::SourceId(Cmp::Equal, id))),
        &state.dbpool,
    )
    .await?;
    let src = Source::fetch(id, &state.dbpool).await?;

    Ok((
        headers,
        RenderHtml(
            key,
            state.tmpl.clone(),
            context!(source => src,
             message => message,
             request => request,
             samples => samples
            ),
        )
        .into_response(),
    ))
}

async fn do_insert(
    user: &SqliteUser,
    params: &SourceParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
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
    source.insert(&state.dbpool).await.map_err(|e| e.into())
}

async fn new_source(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<SourceParams>,
) -> Result<impl IntoResponse, error::Error> {
    let message;
    let mut request: Option<&SourceParams> = None;
    let mut headers = HeaderMap::new();
    match do_insert(&user, &params, &state).await {
        Err(e) => {
            message = Some(Message {
                r#type: MessageType::Error,
                msg: e.to_string(),
            });
            request = Some(&params)
        }
        Ok(result) => {
            let newid = result.last_insert_rowid();
            let url = app_url(&format!("/source/{newid}"));
            message = Some(Message {
                r#type: MessageType::Success,
                msg: format!("Successfully added source {newid}"),
            });
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
        }
    };
    Ok((
        headers,
        RenderHtml(
            key,
            state.tmpl.clone(),
            context!(message => message,
            request => request,
            ),
        ),
    )
        .into_response())
}

async fn delete_source(
    user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let src = Source::fetch(id, &state.dbpool).await?;
    if src.userid != user.id {
        return Err(error::Error::Unauthorized("Not yours".to_string()));
    }
    sqlx::query("DELETE FROM sc_sources WHERE srcid=?")
        .bind(id)
        .execute(&state.dbpool)
        .await?;
    Ok([("HX-redirect", app_url("/source/list"))])
}

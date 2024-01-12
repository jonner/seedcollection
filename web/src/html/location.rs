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
    location,
};
use libseed::{
    location::Location,
    sample::{Filter, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;
use std::sync::Arc;

use crate::{error, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/new", get(add_location).post(new_location))
        .route("/filter", get(filter_locations))
        .route("/new/modal", get(add_location))
        .route(
            "/:id",
            get(show_location)
                .put(update_location)
                .delete(delete_location),
        )
        .route("/:id/edit", get(show_location))
        .route("/list", get(list_locations))
        .route("/list/options", get(list_locations))
}

async fn list_locations(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let locations = Location::fetch_all_user(user.id, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user, locations => locations),
    )
    .into_response())
}

async fn add_location(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => user)).into_response())
}

async fn show_location(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let loc = Location::fetch(id, &state.dbpool).await?;
    let samples = Sample::fetch_all_user(
        user.id,
        Some(Arc::new(Filter::Location(Cmp::Equal, id))),
        &state.dbpool,
    )
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 location => loc,
                 map_viewer => loc.map_viewer_uri(12.0),
                 samples => samples),
    )
    .into_response())
}

#[derive(Debug, Deserialize, Serialize)]
struct LocationParams {
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
    params: &LocationParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_none() {
        return Err(anyhow!("No name specified").into());
    }

    sqlx::query(
        "UPDATE sc_locations SET name=?, description=?, latitude=?, longitude=? WHERE locid=?",
    )
    .bind(params.name.clone())
    .bind(params.description.clone())
    .bind(params.latitude)
    .bind(params.longitude)
    .bind(id)
    .execute(&state.dbpool)
    .await
    .map_err(|e| e.into())
}

async fn update_location(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<LocationParams>,
) -> Result<impl IntoResponse, error::Error> {
    let loc = Location::fetch(id, &state.dbpool).await?;
    if loc.userid != Some(user.id) {
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
                msg: "Successfully updated location".to_string(),
            },
            Some([("HX-Redirect", app_url(&format!("/location/{id}")))]),
        ),
    };
    let samples = Sample::fetch_all_user(
        user.id,
        Some(Arc::new(Filter::Location(Cmp::Equal, id))),
        &state.dbpool,
    )
    .await?;
    let loc = Location::fetch(id, &state.dbpool).await?;

    Ok((
        headers,
        RenderHtml(
            key,
            state.tmpl.clone(),
            context!(location => loc,
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
    params: &LocationParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_none() {
        return Err(anyhow!("No name was given").into());
    }
    sqlx::query(
        r#"INSERT INTO sc_locations
          (name, description, latitude, longitude, userid)
          VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(&params.name)
    .bind(&params.description)
    .bind(params.latitude)
    .bind(params.longitude)
    .bind(user.id)
    .execute(&state.dbpool)
    .await
    .map_err(|e| e.into())
}

async fn new_location(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<LocationParams>,
) -> Result<impl IntoResponse, error::Error> {
    let message;
    let mut request: Option<&LocationParams> = None;
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
            let url = app_url(&format!("/location/{newid}"));
            message = Some(Message {
                r#type: MessageType::Success,
                msg: format!("Successfully added location {newid}"),
            });
            if params.modal.is_some() {
                headers.append(
                    "HX-Trigger",
                    "reload-locations"
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

async fn delete_location(
    user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let loc = Location::fetch(id, &state.dbpool).await?;
    if loc.userid != Some(user.id) {
        return Err(error::Error::Unauthorized("Not yours".to_string()));
    }
    sqlx::query("DELETE FROM sc_locations WHERE locid=?")
        .bind(id)
        .execute(&state.dbpool)
        .await?;
    Ok([("HX-redirect", app_url("/location/list"))])
}

#[derive(Deserialize)]
struct FilterParams {
    fragment: String,
}

async fn filter_locations(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(params): Query<FilterParams>,
) -> Result<impl IntoResponse, error::Error> {
    let namefilter = FilterBuilder::new(FilterOp::Or)
        .push(Arc::new(location::Filter::Name(
            Cmp::Like,
            params.fragment.clone(),
        )))
        .push(Arc::new(location::Filter::Description(
            Cmp::Like,
            params.fragment.clone(),
        )))
        .build();
    let filter = FilterBuilder::new(FilterOp::And)
        .push(namefilter)
        .push(Arc::new(location::Filter::User(user.id)))
        .build();
    let locations = Location::fetch_all(Some(filter), &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user, locations => locations),
    )
    .into_response())
}

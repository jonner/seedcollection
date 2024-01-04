use crate::{
    app_url,
    auth::{AuthSession, SqliteAuthBackend},
    CustomKey, Message, MessageType,
};
use anyhow::anyhow;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::IntoResponse,
    routing::{get, post, put},
    Form, Router,
};
use axum_login::login_required;
use axum_template::RenderHtml;
use libseed::{empty_string_as_none, filter::Cmp};
use libseed::{
    location::Location,
    sample::{Filter, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;

use crate::{error, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/new", get(add_location))
        .route("/new/modal", get(add_location))
        .route("/new", post(new_location))
        .route("/:id", put(update_location).delete(delete_location))
        /* Anything above here is only available to logged-in users */
        .route_layer(login_required!(
            SqliteAuthBackend,
            login_url = app_url("/auth/login")
        ))
        .route("/", get(root))
        .route("/list", get(list_locations))
        .route("/list/options", get(list_locations))
        .route("/:id", get(show_location))
        .route("/:id/edit", get(show_location))
}

async fn root(
    auth_session: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth_session.user),
    ))
}

async fn list_locations(
    auth_session: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let locations = Location::query(&state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth_session.user, locations => locations),
    ))
}

async fn add_location(
    auth_session: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth_session.user),
    ))
}

async fn show_location(
    auth_session: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let loc = Location::fetch(id, &state.dbpool).await?;
    let samples = Sample::query(
        Some(Box::new(Filter::Location(Cmp::Equal, id))),
        &state.dbpool,
    )
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth_session.user,
                 location => loc,
                 map_viewer => loc.map_viewer_uri(12.0),
                 samples => samples),
    ))
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
        "UPDATE seedlocations SET name=?, description=?, latitude=?, longitude=? WHERE locid=?",
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
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<LocationParams>,
) -> Result<impl IntoResponse, error::Error> {
    let samples = Sample::query(
        Some(Box::new(Filter::Location(Cmp::Equal, id))),
        &state.dbpool,
    )
    .await?;
    let mut request: Option<&LocationParams> = None;
    let message = match do_update(id, &params, &state).await {
        Err(e) => {
            request = Some(&params);
            Message {
                r#type: MessageType::Error,
                msg: e.0.to_string(),
            }
        }
        Ok(_) => Message {
            r#type: MessageType::Success,
            msg: "Successfully updated location".to_string(),
        },
    };
    let loc = Location::fetch(id, &state.dbpool).await?;

    Ok(RenderHtml(
        key + ".partial",
        state.tmpl.clone(),
        context!(location => loc,
         message => message,
         request => request,
         samples => samples
        ),
    )
    .into_response())
}

async fn do_insert(
    params: &LocationParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_none() {
        return Err(anyhow!("No name was given").into());
    }
    sqlx::query(
        r#"INSERT INTO seedlocations
          (name, description, latitude, longitude)
          VALUES (?, ?, ?, ?)"#,
    )
    .bind(&params.name)
    .bind(&params.description)
    .bind(params.latitude)
    .bind(params.longitude)
    .execute(&state.dbpool)
    .await
    .map_err(|e| e.into())
}

async fn new_location(
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Form(params): Form<LocationParams>,
) -> Result<impl IntoResponse, error::Error> {
    let message;
    let mut request: Option<&LocationParams> = None;
    let mut headers = HeaderMap::new();
    match do_insert(&params, &state).await {
        Err(e) => {
            message = Some(Message {
                r#type: MessageType::Error,
                msg: e.0.to_string(),
            });
            request = Some(&params)
        }
        Ok(result) => {
            let newid = result.last_insert_rowid();
            let url = app_url(&format!("/location/{newid}"));
            message = Some(Message {
                r#type: MessageType::Success,
                msg: format!(r#"Successfully added location <a href="{url}">{newid}</a>"#),
            });
            headers.append("HX-Trigger", "reload-locations".parse()?);
        }
    };
    Ok((
        headers,
        RenderHtml(
            key + ".partial",
            state.tmpl.clone(),
            context!(message => message,
            request => request,
            ),
        ),
    )
        .into_response())
}

async fn delete_location(
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    sqlx::query("DELETE FROM seedlocations WHERE locid=?")
        .bind(id)
        .execute(&state.dbpool)
        .await?;
    Ok([("HX-Trigger", "reload-locations")])
}

use crate::{
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json},
    routing::{get, post},
    Form, Router,
};
use libseed::{empty_string_as_none, source::Source};
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/list", get(list_sources))
        .route("/new", post(add_source))
        .route(
            "/:id",
            get(show_source).put(modify_source).delete(delete_source),
        )
}

async fn list_sources(
    user: SqliteUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let sources = Source::fetch_all_user(user.id, &state.dbpool).await?;
    Ok(Json(sources).into_response())
}

async fn show_source(
    _user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<Source>, error::Error> {
    let source = Source::fetch(id, &state.dbpool).await?;
    Ok(Json(source))
}

#[derive(Deserialize)]
struct ModifyParams {
    #[serde(deserialize_with = "empty_string_as_none")]
    name: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    description: Option<String>,
    #[serde(deserialize_with = "empty_string_as_none")]
    latitude: Option<f64>,
    #[serde(deserialize_with = "empty_string_as_none")]
    longitude: Option<f64>,
}

async fn modify_source(
    user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Form(params): Form<ModifyParams>,
) -> Result<(), error::Error> {
    if params.name.is_none()
        && params.description.is_none()
        && params.latitude.is_none()
        && params.longitude.is_none()
    {
        return Err(anyhow!("No parameters given").into());
    }
    let mut src = Source::fetch(id, &state.dbpool).await?;
    if src.userid != Some(user.id) {
        return Err(Error::Unauthorized(
            "No permission to modify this source".to_string(),
        ));
    }

    if let Some(name) = params.name {
        src.name = name;
    }
    if let Some(desc) = params.description {
        src.description = Some(desc);
    }
    if let Some(n) = params.latitude {
        src.latitude = Some(n);
    }
    if let Some(n) = params.longitude {
        src.longitude = Some(n);
    }
    src.update(&state.dbpool).await?;
    Ok(())
}

#[derive(Serialize)]
struct AddResponse {
    success: bool,
    id: i64,
}

async fn add_source(
    user: SqliteUser,
    State(state): State<AppState>,
    Form(params): Form<ModifyParams>,
) -> Result<impl IntoResponse, error::Error> {
    if params.name.is_none()
        && params.description.is_none()
        && params.latitude.is_none()
        && params.longitude.is_none()
    {
        return Err(anyhow!("No parameters given").into());
    }
    let mut source = Source::new(
        params.name.ok_or(anyhow!("No name given"))?,
        params.description,
        params.latitude,
        params.longitude,
        Some(user.id),
    );
    let id = source.insert(&state.dbpool).await?.last_insert_rowid();
    Ok((
        [("HX-Trigger", "reload-sources")],
        Json(AddResponse { success: true, id }),
    ))
}

async fn delete_source(
    _user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    sqlx::query("DELETE FROM sc_sources WHERE srcid=?")
        .bind(id)
        .execute(&state.dbpool)
        .await?;
    Ok([("HX-Trigger", "reload-sources")])
}

use crate::{
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Form, Router,
};
use libseed::{empty_string_as_none, location::Location};
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_locations))
        .route("/new", post(add_location))
        .route(
            "/:id",
            get(show_location)
                .put(modify_location)
                .delete(delete_location),
        )
}

async fn root() -> Html<String> {
    Html("Locations".to_string())
}

async fn list_locations(
    user: SqliteUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let locations = Location::fetch_all_user(user.id, &state.dbpool).await?;
    Ok(Json(locations).into_response())
}

async fn show_location(
    _user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<Location>, error::Error> {
    let location = Location::fetch(id, &state.dbpool).await?;
    Ok(Json(location))
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

async fn modify_location(
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
    let mut loc = Location::fetch(id, &state.dbpool).await?;
    if loc.userid != Some(user.id) {
        return Err(Error::Unauthorized(
            "No permission to modify this location".to_string(),
        ));
    }

    if let Some(name) = params.name {
        loc.name = name;
    }
    if let Some(desc) = params.description {
        loc.description = Some(desc);
    }
    if let Some(n) = params.latitude {
        loc.latitude = Some(n);
    }
    if let Some(n) = params.longitude {
        loc.longitude = Some(n);
    }
    loc.update(&state.dbpool).await?;
    Ok(())
}

#[derive(Serialize)]
struct AddResponse {
    success: bool,
    id: i64,
}

async fn add_location(
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
    let location = Location::new(
        params.name.ok_or(anyhow!("No name given"))?,
        params.description,
        params.latitude,
        params.longitude,
        Some(user.id),
    );
    let id = location.insert(&state.dbpool).await?.last_insert_rowid();
    Ok((
        [("HX-Trigger", "reload-locations")],
        Json(AddResponse { success: true, id }),
    ))
}

async fn delete_location(
    _user: SqliteUser,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    sqlx::query("DELETE FROM sc_locations WHERE locid=?")
        .bind(id)
        .execute(&state.dbpool)
        .await?;
    Ok([("HX-Trigger", "reload-locations")])
}

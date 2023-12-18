use crate::{error, state::SharedState};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, Query, State},
    response::{Html, Json},
    routing::{get, post},
    Form, Router,
};
use libseed::location::{self, Location};
use serde::{Deserialize, Serialize};

pub fn router() -> Router<SharedState> {
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
    State(state): State<SharedState>,
) -> Result<Json<Vec<Location>>, error::Error> {
    let locations: Vec<location::Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations",
    )
    .fetch_all(&state.dbpool)
    .await?;
    Ok(Json(locations))
}

async fn show_location(
    Path(id): Path<i64>,
    State(state): State<SharedState>,
) -> Result<Json<Location>, error::Error> {
    let location: Location = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations WHERE locid=?",
    ).bind(id)
    .fetch_one(&state.dbpool)
    .await?;
    Ok(Json(location))
}

#[derive(Deserialize)]
struct ModifyParams {
    name: Option<String>,
    description: Option<String>,
    latitude: Option<f32>,
    longitude: Option<f32>,
}

async fn modify_location(
    Path(id): Path<i64>,
    State(state): State<SharedState>,
    Form(params): Form<ModifyParams>,
) -> Result<(), error::Error> {
    if params.name.is_none()
        && params.description.is_none()
        && params.latitude.is_none()
        && params.longitude.is_none()
    {
        return Err(anyhow!("No parameters given").into());
    }
    let mut builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new("UPDATE seedlocations SET ");
    let mut sep = builder.separated(", ");
    if let Some(name) = params.name {
        sep.push(" name=");
        sep.push_bind_unseparated(name);
    }
    if let Some(desc) = params.description {
        sep.push(" description=");
        sep.push_bind_unseparated(desc);
    }
    if let Some(n) = params.latitude {
        sep.push(" latitude=");
        sep.push_bind_unseparated(n);
    }
    if let Some(n) = params.longitude {
        sep.push(" longitude=");
        sep.push_bind_unseparated(n);
    }
    builder.push(" WHERE locid=");
    builder.push_bind(id);
    builder.build().execute(&state.dbpool).await?;
    Ok(())
}

#[derive(Serialize)]
struct AddResponse {
    success: bool,
    id: i64,
}

async fn add_location(
    Query(params): Query<ModifyParams>,
    State(state): State<SharedState>,
) -> Result<Json<AddResponse>, error::Error> {
    if params.name.is_none()
        && params.description.is_none()
        && params.latitude.is_none()
        && params.longitude.is_none()
    {
        return Err(anyhow!("No parameters given").into());
    }
    let id = sqlx::query(
        r#"INSERT INTO seedlocations
          (name, description, latitude, longitude)
          VALUES (?, ?, ?, ?)"#,
    )
    .bind(params.name)
    .bind(params.description)
    .bind(params.latitude)
    .bind(params.longitude)
    .execute(&state.dbpool)
    .await?
    .last_insert_rowid();
    Ok(Json(AddResponse { success: true, id }))
}

async fn delete_location(
    Path(id): Path<i64>,
    State(state): State<SharedState>,
) -> Result<(), error::Error> {
    sqlx::query("DELETE FROM seedlocations WHERE locid=?")
        .bind(id)
        .execute(&state.dbpool)
        .await?;
    Ok(())
}

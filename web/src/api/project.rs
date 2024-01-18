use crate::{
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Form, Router,
};
use libseed::{
    loadable::Loadable,
    project::{Filter, Project},
    sample::Sample,
};
use serde::Deserialize;
use std::sync::Arc;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/list", get(list_projects))
        .route("/new", post(add_project))
        .route("/:id/sample/:sampleid", delete(remove_sample))
        .route("/:id/add", post(add_sample))
        .route(
            "/:id",
            get(show_project).put(modify_project).delete(delete_project),
        )
}

async fn list_projects(
    user: SqliteUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let projects = Project::fetch_all(Some(Arc::new(Filter::User(user.id))), &state.dbpool).await?;
    Ok(Json(projects).into_response())
}

async fn show_project(
    _user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Project>, error::Error> {
    let mut project = Project::fetch(id, &state.dbpool).await?;
    project.fetch_samples(None, &state.dbpool).await?;
    Ok(Json(project))
}

#[derive(Deserialize)]
struct ModifyProps {
    name: Option<String>,
    description: Option<String>,
}

async fn modify_project(
    user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<ModifyProps>,
) -> Result<(), error::Error> {
    if params.name.is_none() && params.description.is_none() {
        return Err(anyhow!("No params to modify").into());
    }
    let mut project = Project::fetch(id, &state.dbpool).await?;
    if project.userid != user.id {
        return Err(Error::Unauthorized(
            "No permission to modify this project".to_string(),
        ));
    }
    if let Some(name) = params.name {
        project.name = name;
    }
    if let Some(desc) = params.description {
        project.description = Some(desc);
    }
    project.update(&state.dbpool).await?;
    Ok(())
}

async fn delete_project(
    user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(), error::Error> {
    let mut c = Project::fetch(id, &state.dbpool)
        .await
        .map_err(|_| Error::NotFound("That project does not exist".to_string()))?;
    if c.userid != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete this project".to_string(),
        ));
    }

    c.delete(&state.dbpool).await?;
    Ok(())
}

#[derive(Deserialize)]
struct AddProps {
    name: String,
    description: Option<String>,
}

async fn add_project(
    user: SqliteUser,
    Query(params): Query<AddProps>,
    State(state): State<AppState>,
) -> Result<Json<i64>, error::Error> {
    let mut project = Project::new(params.name, params.description, user.id);
    let id = project.insert(&state.dbpool).await?.last_insert_rowid();
    Ok(Json(id))
}

#[derive(Deserialize)]
struct RemoveSampleParams {
    id: i64,
    sampleid: i64,
}

async fn remove_sample(
    _user: SqliteUser,
    State(state): State<AppState>,
    Path(RemoveSampleParams { id, sampleid }): Path<RemoveSampleParams>,
) -> Result<(), error::Error> {
    sqlx::query("DELETE FROM sc_project_samples WHERE projectid=? AND sampleid=?")
        .bind(id)
        .bind(sampleid)
        .execute(&state.dbpool)
        .await?;
    Ok(())
}

#[derive(Deserialize)]
struct AddSampleProps {
    sample: i64,
}

async fn add_sample(
    _user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<AddSampleProps>,
) -> Result<Json<i64>, error::Error> {
    let mut project = Project::fetch(id, &state.dbpool).await?;
    let sample = Sample::new_loadable(params.sample);
    let id = project
        .allocate_sample(sample, &state.dbpool)
        .await?
        .last_insert_rowid();
    Ok(Json(id))
}

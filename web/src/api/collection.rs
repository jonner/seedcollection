use crate::{
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
};
use anyhow::{anyhow, Result};
use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Json},
    routing::{delete, get, post},
    Form, Router,
};
use libseed::{
    collection::{Collection, Filter},
    loadable::Loadable,
    sample::Sample,
};
use serde::Deserialize;
use std::sync::Arc;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_collections))
        .route("/new", post(add_collection))
        .route("/:id/sample/:sampleid", delete(remove_sample))
        .route("/:id/add", post(add_sample))
        .route(
            "/:id",
            get(show_collection)
                .put(modify_collection)
                .delete(delete_collection),
        )
}

async fn root() -> Html<String> {
    Html("Collections".to_string())
}

async fn list_collections(
    user: SqliteUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let collections =
        Collection::fetch_all(Some(Arc::new(Filter::User(user.id))), &state.dbpool).await?;
    Ok(Json(collections).into_response())
}

async fn show_collection(
    _user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Collection>, error::Error> {
    let mut collection = Collection::fetch(id, &state.dbpool).await?;
    collection.fetch_samples(None, &state.dbpool).await?;
    Ok(Json(collection))
}

#[derive(Deserialize)]
struct ModifyProps {
    name: Option<String>,
    description: Option<String>,
}

async fn modify_collection(
    user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<ModifyProps>,
) -> Result<(), error::Error> {
    if params.name.is_none() && params.description.is_none() {
        return Err(anyhow!("No params to modify").into());
    }
    let mut collection = Collection::fetch(id, &state.dbpool).await?;
    if collection.userid != user.id {
        return Err(Error::Unauthorized(
            "No permission to modify this collection".to_string(),
        ));
    }
    if let Some(name) = params.name {
        collection.name = name;
    }
    if let Some(desc) = params.description {
        collection.description = Some(desc);
    }
    collection.update(&state.dbpool).await?;
    Ok(())
}

async fn delete_collection(
    user: SqliteUser,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<(), error::Error> {
    let mut c = Collection::fetch(id, &state.dbpool)
        .await
        .map_err(|_| Error::NotFound("That collection does not exist".to_string()))?;
    if c.userid != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete this collection".to_string(),
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

async fn add_collection(
    user: SqliteUser,
    Query(params): Query<AddProps>,
    State(state): State<AppState>,
) -> Result<Json<i64>, error::Error> {
    let mut collection = Collection::new(params.name, params.description, user.id);
    let id = collection.insert(&state.dbpool).await?.last_insert_rowid();
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
    sqlx::query("DELETE FROM sc_collection_samples WHERE collectionid=? AND sampleid=?")
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
    let mut collection = Collection::fetch(id, &state.dbpool).await?;
    let sample = Sample::new_loadable(params.sample);
    let id = collection
        .allocate_sample(sample, &state.dbpool)
        .await?
        .last_insert_rowid();
    Ok(Json(id))
}

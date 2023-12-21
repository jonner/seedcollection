use axum::Router;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
};
use axum_template::RenderHtml;
use libseed::collection::Collection;
use libseed::sample::{self, Filter, Sample};
use minijinja::context;
use serde::Deserialize;

use crate::CustomKey;
use crate::{error, state::SharedState};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_collections))
        .route("/new", post(add_collection))
        .route("/:id/add", get(add_sample))
        .route(
            "/:id",
            get(show_collection)
                .put(modify_collection)
                .delete(delete_collection),
        )
}

async fn root() -> impl IntoResponse {
    "Collections"
}

async fn list_collections(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let collections: Vec<Collection> =
        sqlx::query_as("SELECT L.id, L.name, L.description FROM seedcollections L")
            .fetch_all(&state.dbpool)
            .await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(collections => collections),
    ))
}

async fn add_collection() -> impl IntoResponse {
    "Add collection"
}

async fn show_collection(
    CustomKey(key): CustomKey,
    Path(id): Path<i64>,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut c: Collection =
        sqlx::query_as("SELECT L.id, L.name, L.description FROM seedcollections L WHERE id=?")
            .bind(id)
            .fetch_one(&state.dbpool)
            .await?;
    let mut builder = sample::build_query(Some(Filter::Collection(id)));
    c.samples = builder.build_query_as().fetch_all(&state.dbpool).await?;

    Ok(RenderHtml(key, state.tmpl, context!(collection => c)))
}

async fn modify_collection() -> impl IntoResponse {
    "Modify collection"
}

async fn delete_collection() -> impl IntoResponse {
    "Delete collection"
}

async fn add_sample(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let mut c: Collection =
        sqlx::query_as("SELECT L.id, L.name, L.description FROM seedcollections L WHERE id=?")
            .bind(id)
            .fetch_one(&state.dbpool)
            .await?;
    let mut builder = sample::build_query(Some(Filter::Collection(id)));
    c.samples = builder.build_query_as().fetch_all(&state.dbpool).await?;

    let options: Vec<Sample> = sample::build_query(Some(Filter::NoCollection))
        .build_query_as()
        .fetch_all(&state.dbpool)
        .await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(collection => c, options => options),
    ))
}

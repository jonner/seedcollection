use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
};
use axum::{Form, Router};
use axum_template::RenderHtml;
use libseed::collection::Collection;
use libseed::sample::{self, Filter, Sample};
use log::debug;
use minijinja::context;

use crate::CustomKey;
use crate::{error, state::SharedState};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_collections))
        .route("/new", post(add_collection))
        .route("/:id/add", get(show_add_sample).post(add_sample))
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

async fn show_add_sample(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let c: Collection =
        sqlx::query_as("SELECT id, name, description FROM seedcollections WHERE id=?")
            .bind(id)
            .fetch_one(&state.dbpool)
            .await?;

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

async fn add_sample(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Path(id): Path<i64>,
    Form(params): Form<Vec<(String, String)>>,
) -> Result<impl IntoResponse, error::Error> {
    let samples: Vec<i64> = params
        .iter()
        .filter_map(|(name, value)| match name.as_str() {
            "sample" => value.parse::<i64>().ok(),
            _ => None,
        })
        .collect();

    for sample in samples {
        sqlx::query("INSERT INTO seedcollectionsamples (collectionid, sampleid) VALUES (?, ?)")
            .bind(id)
            .bind(sample)
            .execute(&state.dbpool)
            .await?;
    }

    let c: Collection =
        sqlx::query_as("SELECT id, name, description FROM seedcollections WHERE id=?")
            .bind(id)
            .fetch_one(&state.dbpool)
            .await?;
    let options: Vec<Sample> = sample::build_query(Some(Filter::NoCollection))
        .build_query_as()
        .fetch_all(&state.dbpool)
        .await?;
    Ok(RenderHtml(
        key + ".partial",
        state.tmpl,
        context!(collection => c, options => options, partial => true),
    ))
}

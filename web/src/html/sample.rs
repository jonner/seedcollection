use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use axum_template::RenderHtml;
use libseed::{
    collection::Collection,
    location::Location,
    sample::{self, Filter, Sample},
};
use minijinja::context;

use crate::{error, state::SharedState, CustomKey};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/list", get(list_samples))
        .route("/:id", get(show_sample))
        .route("/new", get(new_sample))
}

async fn list_samples(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(None);
    let samples: Vec<Sample> = builder.build_query_as().fetch_all(&state.dbpool).await?;
    Ok(RenderHtml(key, state.tmpl, context!(samples => samples)))
}

async fn show_sample(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let mut builder = sample::build_query(Some(Filter::Sample(id)));
    let sample: Sample = builder.build_query_as().fetch_one(&state.dbpool).await?;
    let collection = match sample.collection {
        Some(cid) => {
            let c: Collection =
                sqlx::query_as("SELECT id, name, description FROM seedcollections WHERE id=?")
                    .bind(cid)
                    .fetch_one(&state.dbpool)
                    .await?;
            Some(c)
        }
        None => None,
    };
    let locations: Vec<Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations ORDER BY name ASC",
    )
    .fetch_all(&state.dbpool)
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(sample => sample, locations => locations, collection => collection),
    ))
}

async fn new_sample(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let locations: Vec<Location> = sqlx::query_as(
        "SELECT locid, name as locname, description, latitude, longitude FROM seedlocations ORDER BY name ASC",
    )
    .fetch_all(&state.dbpool)
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(locations => locations),
    ))
}

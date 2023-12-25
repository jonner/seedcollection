use anyhow::anyhow;
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    collection::Collection,
    empty_string_as_none,
    sample::{self, Filter, Sample},
};
use log::debug;
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;

use crate::{app_url, error, state::SharedState, CustomKey, Message, MessageType};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/", get(root))
        .route("/list", get(list_collections))
        .route("/new", get(new_collection).post(insert_collection))
        .route("/:id/add", get(show_add_sample).post(add_sample))
        .route(
            "/:id",
            get(show_collection)
                .put(modify_collection)
                .delete(delete_collection),
        )
}

async fn root(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl, ()))
}

async fn list_collections(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let collections: Vec<Collection> =
        sqlx::query_as("SELECT id, name, description FROM seedcollections")
            .fetch_all(&state.dbpool)
            .await?;
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(collections => collections),
    ))
}

async fn new_collection(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl, ()))
}

#[derive(Deserialize, Serialize)]
struct CollectionParams {
    name: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    description: Option<String>,
}

async fn do_insert(
    params: &CollectionParams,
    state: &SharedState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_empty() {
        return Err(anyhow!("No name specified").into());
    }
    sqlx::query("INSERT INTO seedcollections (name, description) VALUES (?, ?)")
        .bind(params.name.clone())
        .bind(params.description.clone())
        .execute(&state.dbpool)
        .await
        .map_err(|e| e.into())
}

async fn insert_collection(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
    Form(params): Form<CollectionParams>,
) -> Result<impl IntoResponse, error::Error> {
    match do_insert(&params, &state).await {
        Err(e) => Ok(RenderHtml(
            key + ".partial",
            state.tmpl,
            context!( message => Message {
                r#type: MessageType::Error,
                msg: format!("Failed to save collection: {}", e.0.to_string())
            },
            request => params),
        )),
        Ok(result) => {
            let id = result.last_insert_rowid();
            let collection: Collection =
                sqlx::query_as("SELECT id, name, description FROM seedcollections WHERE id=?")
                    .bind(id)
                    .fetch_one(&state.dbpool)
                    .await?;

            let collectionurl = app_url(format!("/collection/{}", collection.id));
            Ok(RenderHtml(
                key + ".partial",
                state.tmpl,
                context!(message => Message {
                    r#type: MessageType::Success,
                    msg: format!(r#"Added new collection <a href="{}">{}: {}</a> to the database"#,
                                 collectionurl,
                                 collection.id, collection.name)
                }),
            ))
        }
    }
}

async fn show_collection(
    CustomKey(key): CustomKey,
    Path(id): Path<i64>,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut c: Collection =
        sqlx::query_as("SELECT id, name, description FROM seedcollections WHERE id=?")
            .bind(id)
            .fetch_one(&state.dbpool)
            .await?;
    let mut builder = sample::build_query(Some(Filter::Collection(id)));
    c.samples = builder.build_query_as().fetch_all(&state.dbpool).await?;

    Ok(RenderHtml(key, state.tmpl, context!(collection => c)))
}

async fn do_update(
    id: i64,
    params: &CollectionParams,
    state: &SharedState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_empty() {
        return Err(anyhow!("No name specified").into());
    }
    sqlx::query("UPDATE seedcollections SET name=?, description=? WHERE id=?")
        .bind(params.name.clone())
        .bind(params.description.clone())
        .bind(id)
        .execute(&state.dbpool)
        .await
        .map_err(|e| e.into())
}

async fn modify_collection(
    CustomKey(key): CustomKey,
    Path(id): Path<i64>,
    State(state): State<SharedState>,
    Form(params): Form<CollectionParams>,
) -> Result<impl IntoResponse, error::Error> {
    match do_update(id, &params, &state).await {
        Err(e) => {
            let c: Collection =
                sqlx::query_as("SELECT id, name, description FROM seedcollections WHERE id=?")
                    .bind(id)
                    .fetch_one(&state.dbpool)
                    .await?;
            Ok(RenderHtml(
                key + ".partial",
                state.tmpl,
                context!(collection => c,
                 message => Message {
                     r#type: MessageType::Error,
                     msg: e.0.to_string(),
                 }
                ),
            ))
        }
        Ok(_) => {
            let c: Collection =
                sqlx::query_as("SELECT id, name, description FROM seedcollections WHERE id=?")
                    .bind(id)
                    .fetch_one(&state.dbpool)
                    .await?;
            Ok(RenderHtml(
                key + ".partial",
                state.tmpl,
                context!(collection => c,
                message => Message {
                    r#type: MessageType::Success,
                    msg: "Successfully updated collection".to_string(),
                }),
            ))
        }
    }
}

async fn delete_collection(
    CustomKey(key): CustomKey,
    Path(id): Path<i64>,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    match sqlx::query!("DELETE FROM seedcollections WHERE id=?", id)
        .execute(&state.dbpool)
        .await
    {
        Err(e) => {
            let c: Collection =
                sqlx::query_as("SELECT id, name, description FROM seedcollections WHERE id=?")
                    .bind(id)
                    .fetch_one(&state.dbpool)
                    .await?;
            Ok(RenderHtml(
                key + ".partial",
                state.tmpl,
                context!(
                collection => c,
                message => Message {
                    r#type: MessageType::Error,
                    msg: format!("Failed to delete collection: {}", e.to_string())
                },
                ),
            ))
        }
        Ok(_) => Ok(RenderHtml(
            key + ".partial",
            state.tmpl,
            context!(deleted => true, id => id),
        )),
    }
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

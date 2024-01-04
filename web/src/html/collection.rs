use anyhow::anyhow;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get},
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    collection::Collection,
    empty_string_as_none,
    sample::{Filter, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;

use crate::{app_url, auth::AuthSession, error, state::AppState, CustomKey, Message, MessageType};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(root))
        .route("/new", get(new_collection).post(insert_collection))
        .route("/list", get(list_collections))
        .route(
            "/:id",
            get(show_collection)
                .put(modify_collection)
                .delete(delete_collection),
        )
        .route("/:id/edit", get(show_collection))
        .route("/:id/add", get(show_add_sample).post(add_sample))
        .route("/:id/sample/:sampleid", delete(remove_sample))
}

async fn root(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user),
    ))
}

async fn list_collections(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let collections = Collection::fetch_all(&state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 collections => collections),
    ))
}

async fn new_collection(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => auth.user)).into_response())
}

#[derive(Deserialize, Serialize)]
struct CollectionParams {
    name: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    description: Option<String>,
}

async fn do_insert(
    params: &CollectionParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_empty() {
        return Err(anyhow!("No name specified").into());
    }
    sqlx::query("INSERT INTO sc_collections (name, description) VALUES (?, ?)")
        .bind(params.name.clone())
        .bind(params.description.clone())
        .execute(&state.dbpool)
        .await
        .map_err(|e| e.into())
}

async fn insert_collection(
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Form(params): Form<CollectionParams>,
) -> Result<impl IntoResponse, error::Error> {
    let (request, message) = match do_insert(&params, &state).await {
        Err(e) => (
            Some(&params),
            Message {
                r#type: MessageType::Error,
                msg: format!("Failed to save collection: {}", e.0),
            },
        ),
        Ok(result) => {
            let id = result.last_insert_rowid();
            let collectionurl = app_url(&format!("/collection/{}", id));
            (
                None,
                Message {
                    r#type: MessageType::Success,
                    msg: format!(
                        r#"Added new collection <a href="{}">{}: {}</a> to the database"#,
                        collectionurl, id, params.name
                    ),
                },
            )
        }
    };

    Ok(RenderHtml(
        key + ".partial",
        state.tmpl.clone(),
        context!( message => message,
            request => request),
    )
    .into_response())
}

async fn show_collection(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let mut c = Collection::fetch(id, &state.dbpool).await?;
    c.fetch_samples(&state.dbpool).await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 collection => c),
    ))
}

async fn do_update(
    id: i64,
    params: &CollectionParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_empty() {
        return Err(anyhow!("No name specified").into());
    }
    sqlx::query("UPDATE sc_collections SET name=?, description=? WHERE id=?")
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
    State(state): State<AppState>,
    Form(params): Form<CollectionParams>,
) -> Result<impl IntoResponse, error::Error> {
    let (request, message) = match do_update(id, &params, &state).await {
        Err(e) => (
            Some(&params),
            Message {
                r#type: MessageType::Error,
                msg: e.0.to_string(),
            },
        ),
        Ok(_) => (
            None,
            Message {
                r#type: MessageType::Success,
                msg: "Successfully updated collection".to_string(),
            },
        ),
    };
    let c = Collection::fetch(id, &state.dbpool).await?;
    Ok(RenderHtml(
        key + ".partial",
        state.tmpl.clone(),
        context!(collection => c,
         message => message,
         request => request,
        ),
    )
    .into_response())
}

async fn delete_collection(
    CustomKey(key): CustomKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    match sqlx::query!("DELETE FROM sc_collections WHERE id=?", id)
        .execute(&state.dbpool)
        .await
    {
        Err(e) => {
            let c = Collection::fetch(id, &state.dbpool).await?;
            Ok(RenderHtml(
                key + ".partial",
                state.tmpl.clone(),
                context!(
                collection => c,
                message => Message {
                    r#type: MessageType::Error,
                    msg: format!("Failed to delete collection: {}", e)
                },
                ),
            )
            .into_response())
        }
        Ok(_) => Ok(RenderHtml(
            key + ".partial",
            state.tmpl.clone(),
            context!(deleted => true, id => id),
        )
        .into_response()),
    }
}

async fn add_sample_prep(
    id: i64,
    state: &AppState,
) -> Result<(Collection, Vec<Sample>), error::Error> {
    let c = Collection::fetch(id, &state.dbpool).await?;

    let ids_in_collection = sqlx::query!(
        "SELECT CS.sampleid from sc_collection_samples CS WHERE CS.collectionid=?",
        id
    )
    .fetch_all(&state.dbpool)
    .await?;
    let ids = ids_in_collection.iter().map(|row| row.sampleid).collect();
    let samples =
        Sample::fetch_all(Some(Box::new(Filter::SampleNotIn(ids))), &state.dbpool).await?;
    Ok((c, samples))
}

async fn show_add_sample(
    auth: AuthSession,
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let (c, samples) = add_sample_prep(id, &state).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 collection => c,
                 samples => samples),
    )
    .into_response())
}

async fn add_sample(
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<Vec<(String, String)>>,
) -> Result<impl IntoResponse, error::Error> {
    let toadd: Vec<i64> = params
        .iter()
        .filter_map(|(name, value)| match name.as_str() {
            "sample" => value.parse::<i64>().ok(),
            _ => None,
        })
        .collect();

    for sample in toadd {
        sqlx::query("INSERT INTO sc_collection_samples (collectionid, sampleid) VALUES (?, ?)")
            .bind(id)
            .bind(sample)
            .execute(&state.dbpool)
            .await?;
    }

    let (c, samples) = add_sample_prep(id, &state).await?;
    Ok(RenderHtml(
        key + ".partial",
        state.tmpl.clone(),
        context!(collection => c,
                 samples => samples),
    )
    .into_response())
}

async fn remove_sample(
    auth: AuthSession,
    State(state): State<AppState>,
    Path((id, sampleid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    if auth.user.is_none() {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    sqlx::query!(
        "DELETE FROM sc_collection_samples WHERE collectionid=? AND sampleid=?",
        id,
        sampleid
    )
    .execute(&state.dbpool)
    .await?;
    Ok(().into_response())
}

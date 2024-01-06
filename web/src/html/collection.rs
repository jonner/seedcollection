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
    collection::{self, Collection},
    empty_string_as_none,
    filter::{CompoundFilter, FilterOp},
    sample::{self, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteQueryResult;
use sqlx::Row;
use tracing::warn;

use crate::{
    app_url,
    auth::{AuthSession, SqliteUser},
    error,
    state::AppState,
    Message, MessageType, TemplateKey,
};

pub fn router() -> Router<AppState> {
    Router::new()
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

async fn list_collections(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let Some(ref user) = auth.user else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };

    let collections = Collection::fetch_all(
        Some(Box::new(collection::Filter::User(user.id))),
        &state.dbpool,
    )
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                     collections => collections),
    )
    .into_response())
}

async fn new_collection(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
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
    user: SqliteUser,
    params: &CollectionParams,
    state: &AppState,
) -> Result<SqliteQueryResult, error::Error> {
    if params.name.is_empty() {
        return Err(anyhow!("No name specified").into());
    }
    sqlx::query("INSERT INTO sc_collections (name, description, userid) VALUES (?, ?, ?)")
        .bind(params.name.clone())
        .bind(params.description.clone())
        .bind(user.id)
        .execute(&state.dbpool)
        .await
        .map_err(|e| e.into())
}

async fn insert_collection(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<CollectionParams>,
) -> Result<impl IntoResponse, error::Error> {
    let Some(user) = auth.user else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let (request, message) = match do_insert(user, &params, &state).await {
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
        key,
        state.tmpl.clone(),
        context!( message => message,
            request => request),
    )
    .into_response())
}

async fn show_collection(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let Some(ref user) = auth.user else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let mut filter = CompoundFilter::new(FilterOp::And);
    filter.add_filter(Box::new(collection::Filter::Id(id)));
    filter.add_filter(Box::new(collection::Filter::User(user.id)));
    let mut collections = Collection::fetch_all(Some(Box::new(filter)), &state.dbpool).await?;
    let Some(mut c) = collections.pop() else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    c.fetch_samples(&state.dbpool).await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user,
                 collection => c),
    )
    .into_response())
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
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Form(params): Form<CollectionParams>,
) -> Result<impl IntoResponse, error::Error> {
    let Some(user) = auth.user else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let mut filter = CompoundFilter::new(FilterOp::And);
    filter.add_filter(Box::new(collection::Filter::Id(id)));
    filter.add_filter(Box::new(collection::Filter::User(user.id)));
    let collections = Collection::fetch_all(Some(Box::new(filter)), &state.dbpool).await?;
    if collections.is_empty() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
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
        key,
        state.tmpl.clone(),
        context!(collection => c,
         message => message,
         request => request,
        ),
    )
    .into_response())
}

async fn delete_collection(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let Some(user) = auth.user else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let errmsg = match sqlx::query!(
        "DELETE FROM sc_collections WHERE id=? AND userid=?",
        id,
        user.id
    )
    .execute(&state.dbpool)
    .await
    {
        Err(e) => e.to_string(),
        Ok(res) if (res.rows_affected() == 0) => "No collection found".to_string(),
        Ok(_) => {
            return Ok(
                RenderHtml(key, state.tmpl.clone(), context!(deleted => true, id => id))
                    .into_response(),
            )
        }
    };
    let c = Collection::fetch(id, &state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(
        collection => c,
        message => Message {
            r#type: MessageType::Error,
            msg: format!("Failed to delete collection: {errmsg}")
        },
        ),
    )
    .into_response())
}

async fn add_sample_prep(
    user: &SqliteUser,
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
    let samples = Sample::fetch_all_user(
        user.id,
        Some(Box::new(sample::Filter::SampleNotIn(ids))),
        &state.dbpool,
    )
    .await?;
    Ok((c, samples))
}

async fn show_add_sample(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
    let Some(user) = auth.user else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let (c, samples) = add_sample_prep(&user, id, &state).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collection => c,
                 samples => samples),
    )
    .into_response())
}

async fn add_sample(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Form(params): Form<Vec<(String, String)>>,
) -> Result<impl IntoResponse, error::Error> {
    let Some(user) = auth.user else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };

    let toadd: Vec<i64> = params
        .iter()
        .filter_map(|(name, value)| match name.as_str() {
            "sample" => value.parse::<i64>().ok(),
            _ => None,
        })
        .collect();
    let res = sqlx::query!("SELECT userid FROM sc_collections WHERE id=?", id)
        .fetch_one(&state.dbpool)
        .await?;
    if res.userid != Some(user.id) {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    let mut qb = sqlx::QueryBuilder::new("SELECT id, userid FROM sc_samples WHERE id IN (");
    let mut sep = qb.separated(", ");
    for id in toadd {
        sep.push_bind(id);
    }
    qb.push(")");
    let res = qb.build().fetch_all(&state.dbpool).await?;
    let valid_samples = res.iter().filter_map(|row| {
        let userid: Option<i64> = row.try_get("userid").ok()?;
        let userid = userid?;
        let id: i64 = row.try_get("id").ok()?;
        if userid == user.id {
            Some(id)
        } else {
            warn!(
                "dropping sample {} which is not owned by user {}",
                id, user.id
            );
            None
        }
    });

    for sample in valid_samples {
        sqlx::query("INSERT INTO sc_collection_samples (collectionid, sampleid) VALUES (?, ?)")
            .bind(id)
            .bind(sample)
            .execute(&state.dbpool)
            .await?;
    }

    let (c, samples) = add_sample_prep(&user, id, &state).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(collection => c,
                 samples => samples),
    )
    .into_response())
}

async fn remove_sample(
    auth: AuthSession,
    State(state): State<AppState>,
    Path((_, csid)): Path<(i64, i64)>,
) -> impl IntoResponse {
    match auth.user {
        Some(user) => match sqlx::query!(
            "DELETE FROM sc_collection_samples AS CS WHERE CS.id=? AND CS.collectionid IN (SELECT C.id FROM sc_collections AS C WHERE C.userid=?)",
                         csid, user.id)
            .execute(&state.dbpool)
            .await {
                Ok(_) => ().into_response(),
                Err(e) => {
                    warn!("Failed to remove sample {} from collection: {}", csid, e);
                    StatusCode::INTERNAL_SERVER_ERROR.into_response()
                },
            }
        None => StatusCode::UNAUTHORIZED.into_response(),
    }
}

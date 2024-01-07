use crate::{
    app_url,
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
    Message, MessageType, TemplateKey,
};
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
    filter::{FilterBuilder, FilterOp},
    sample::{self, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Row};
use std::sync::Arc;
use tracing::warn;

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
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let collections = Collection::fetch_all(
        Some(Arc::new(collection::Filter::User(user.id))),
        &state.dbpool,
    )
    .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collections => collections),
    )
    .into_response())
}

async fn new_collection(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => user)).into_response())
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
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<CollectionParams>,
) -> Result<impl IntoResponse, error::Error> {
    match do_insert(user, &params, &state).await {
        Err(e) => Ok(RenderHtml(
            key,
            state.tmpl.clone(),
            context!( message => Message {
                    r#type: MessageType::Error,
                    msg: format!("Failed to save collection: {}", e),
                },
                request => params),
        )
        .into_response()),
        Ok(result) => {
            let id = result.last_insert_rowid();
            let collectionurl = app_url(&format!("/collection/{}", id));

            Ok((
                [("HX-Redirect", collectionurl)],
                RenderHtml(
                    key,
                    state.tmpl.clone(),
                    context!( message =>
                    Message {
                        r#type: MessageType::Success,
                        msg: format!(
                            r#"Added new collection {}: {} to the database"#,
                            id, params.name
                            )
                    },
                    ),
                ),
            )
                .into_response())
        }
    }
}

async fn show_collection(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let fb = FilterBuilder::new(FilterOp::And)
        .add(Arc::new(collection::Filter::Id(id)))
        .add(Arc::new(collection::Filter::User(user.id)));
    let mut collections = Collection::fetch_all(Some(fb.build()), &state.dbpool).await?;
    let Some(mut c) = collections.pop() else {
        return Err(Error::NotFound(
            "That collection does not exist".to_string(),
        ));
    };
    c.fetch_samples(&state.dbpool).await?;

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
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
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Form(params): Form<CollectionParams>,
) -> Result<impl IntoResponse, error::Error> {
    let fb = FilterBuilder::new(FilterOp::And)
        .add(Arc::new(collection::Filter::Id(id)))
        .add(Arc::new(collection::Filter::User(user.id)));
    let collections = Collection::fetch_all(Some(fb.build()), &state.dbpool).await?;
    if collections.is_empty() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let (request, message) = match do_update(id, &params, &state).await {
        Err(e) => (
            Some(&params),
            Message {
                r#type: MessageType::Error,
                msg: e.to_string(),
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
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let c = Collection::fetch(id, &state.dbpool)
        .await
        .map_err(|_| Error::NotFound("That collection does not exist".to_string()))?;
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
            return Ok((
                [("HX-Redirect", app_url("/collection/list"))],
                RenderHtml(key, state.tmpl.clone(), context!(deleted => true, id => id)),
            )
                .into_response())
        }
    };
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
        Some(Arc::new(sample::Filter::SampleNotIn(ids))),
        &state.dbpool,
    )
    .await?;
    Ok((c, samples))
}

async fn show_add_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, error::Error> {
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
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
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
    user: SqliteUser,
    State(state): State<AppState>,
    Path((id, csid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    let mut collections =
        Collection::fetch_all(Some(Arc::new(collection::Filter::Id(id))), &state.dbpool).await?;
    let Some(c) = collections.pop() else {
        return Err(Error::NotFound(
            "That collection does not exist".to_string(),
        ));
    };
    if c.userid != user.id {
        return Err(Error::NotFound(
            "That collection does not exist".to_string(),
        ));
    }
    sqlx::query!(
        "DELETE FROM sc_collection_samples AS CS WHERE CS.id=? AND CS.collectionid IN (SELECT C.id FROM sc_collections AS C WHERE C.userid=?)",
        csid, user.id)
        .execute(&state.dbpool)
        .await?;
    Ok(())
}

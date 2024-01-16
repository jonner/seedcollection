use crate::{
    app_url,
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
    Message, MessageType, TemplateKey,
};
use anyhow::anyhow;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    collection::{self, AssignedSample, AssignedSampleFilter, Collection},
    empty_string_as_none,
    filter::{Cmp, FilterBuilder, FilterOp},
    loadable::Loadable,
    location,
    note::{Note, NoteType},
    sample::{self, Sample},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Row};
use std::sync::Arc;
use strum::IntoEnumIterator;
use tracing::warn;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/new", get(new_collection).post(insert_collection))
        .route("/list", get(list_collections))
        .route("/filter", get(filter_collections))
        .route(
            "/:id",
            get(show_collection)
                .put(modify_collection)
                .delete(delete_collection),
        )
        .route("/:id/edit", get(show_collection))
        .route("/:id/filter", get(filter_collection_samples))
        .route("/:id/add", get(show_add_sample).post(add_sample))
        .route(
            "/:id/sample/:sampleid",
            get(show_collection_sample).delete(remove_sample),
        )
        .route(
            "/:id/sample/:sampleid/note/new",
            get(show_add_sample_note).post(add_sample_note),
        )
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

#[derive(Deserialize)]
struct FilterParams {
    fragment: String,
}

async fn filter_collections(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Query(params): Query<FilterParams>,
) -> Result<impl IntoResponse, error::Error> {
    let namefilter = FilterBuilder::new(FilterOp::Or)
        .push(Arc::new(collection::Filter::Name(
            Cmp::Like,
            params.fragment.clone(),
        )))
        .push(Arc::new(collection::Filter::Description(
            Cmp::Like,
            params.fragment.clone(),
        )))
        .build();
    let filter = FilterBuilder::new(FilterOp::And)
        .push(namefilter)
        .push(Arc::new(collection::Filter::User(user.id)))
        .build();

    let collections = Collection::fetch_all(Some(filter), &state.dbpool).await?;
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

    let mut collection = Collection::new(
        params.name.clone(),
        params.description.as_ref().cloned(),
        user.id,
    );
    collection.insert(&state.dbpool).await.map_err(|e| e.into())
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
        .push(Arc::new(collection::Filter::Id(id)))
        .push(Arc::new(collection::Filter::User(user.id)));
    let mut collections = Collection::fetch_all(Some(fb.build()), &state.dbpool).await?;
    let Some(mut c) = collections.pop() else {
        return Err(Error::NotFound(
            "That collection does not exist".to_string(),
        ));
    };
    c.fetch_samples(None, &state.dbpool).await?;

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
    let mut collection = Collection::fetch(id, &state.dbpool).await?;
    collection.name = params.name.clone();
    collection.description = params.description.clone();
    collection.update(&state.dbpool).await.map_err(|e| e.into())
}

async fn modify_collection(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    Path(id): Path<i64>,
    State(state): State<AppState>,
    Form(params): Form<CollectionParams>,
) -> Result<impl IntoResponse, error::Error> {
    let fb = FilterBuilder::new(FilterOp::And)
        .push(Arc::new(collection::Filter::Id(id)))
        .push(Arc::new(collection::Filter::User(user.id)));
    let collections = Collection::fetch_all(Some(fb.build()), &state.dbpool).await?;
    if collections.is_empty() {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let (request, message, headers) = match do_update(id, &params, &state).await {
        Err(e) => (
            Some(&params),
            Message {
                r#type: MessageType::Error,
                msg: e.to_string(),
            },
            None,
        ),
        Ok(_) => (
            None,
            Message {
                r#type: MessageType::Success,
                msg: "Successfully updated collection".to_string(),
            },
            Some([("HX-Redirect", app_url(&format!("/collection/{id}")))]),
        ),
    };
    let c = Collection::fetch(id, &state.dbpool).await?;
    Ok((
        headers,
        RenderHtml(
            key,
            state.tmpl.clone(),
            context!(collection => c,
             message => message,
             request => request,
            ),
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
    let mut c = Collection::fetch(id, &state.dbpool)
        .await
        .map_err(|_| Error::NotFound("That collection does not exist".to_string()))?;
    if c.userid != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete this collection".to_string(),
        ));
    }

    let errmsg = match c.delete(&state.dbpool).await {
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

    /* FIXME: make this more efficient by changeing it to filter by
     *  'WHERE NOT IN (SELECT ids from...)'
     * instead of
     * [ query ids first ], then
     *  'WHERE NOT IN (1, 2, 3, 4, 5...)'
     */
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

    let mut collection = Collection::fetch(id, &state.dbpool).await?;
    for sample in valid_samples {
        let s = Sample::new_loadable(sample);
        collection.assign_sample(s, &state.dbpool).await?;
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

async fn filter_collection_samples(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(params): Query<FilterParams>,
) -> Result<impl IntoResponse, error::Error> {
    let fragment = params.fragment.trim();
    let mut fbuilder =
        FilterBuilder::new(FilterOp::And).push(Arc::new(sample::Filter::User(user.id)));
    if !fragment.is_empty() {
        let subfilter = FilterBuilder::new(FilterOp::Or)
            .push(Arc::new(sample::Filter::TaxonNameLike(
                params.fragment.clone(),
            )))
            .push(Arc::new(location::Filter::Name(
                Cmp::Like,
                params.fragment.clone(),
            )))
            .push(Arc::new(sample::Filter::Notes(
                Cmp::Like,
                params.fragment.clone(),
            )))
            .build();
        fbuilder = fbuilder.push(subfilter);
    }

    let mut collection = Collection::fetch(id, &state.dbpool).await?;
    collection
        .fetch_samples(Some(fbuilder.build()), &state.dbpool)
        .await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collection => collection),
    )
    .into_response())
}

async fn show_add_sample_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, sampleid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    let collection = Collection::fetch(collectionid, &state.dbpool).await?;
    let sample = AssignedSample::fetch_one(
        Some(
            FilterBuilder::new(FilterOp::And)
                .push(Arc::new(AssignedSampleFilter::Id(sampleid)))
                .push(Arc::new(AssignedSampleFilter::User(user.id)))
                .push(Arc::new(AssignedSampleFilter::Collection(collectionid)))
                .build(),
        ),
        &state.dbpool,
    )
    .await?;
    let note_types: Vec<NoteType> = NoteType::iter().collect();
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collection => collection,
                 note_types => note_types,
                 sample => sample),
    )
    .into_response())
}

#[derive(Deserialize, Serialize)]
struct NoteParams {
    summary: String,
    date: time::Date,
    notetype: NoteType,
    #[serde(deserialize_with = "empty_string_as_none")]
    details: Option<String>,
}

async fn add_sample_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, sampleid)): Path<(i64, i64)>,
    Form(params): Form<NoteParams>,
) -> Result<impl IntoResponse, error::Error> {
    let _ = Collection::fetch(collectionid, &state.dbpool).await?;
    // make sure that this is our sample
    let sample = AssignedSample::fetch_one(
        Some(
            FilterBuilder::new(FilterOp::And)
                .push(Arc::new(AssignedSampleFilter::Id(sampleid)))
                .push(Arc::new(AssignedSampleFilter::User(user.id)))
                .push(Arc::new(AssignedSampleFilter::Collection(collectionid)))
                .build(),
        ),
        &state.dbpool,
    )
    .await?;
    let note = Note::new(
        sampleid,
        params.date,
        params.notetype,
        params.summary,
        params.details,
    );
    Ok(match note.insert(&state.dbpool).await {
        Ok(_) => {
            let url = app_url(&format!("/collection/{}/sample/{}", collectionid, sampleid));
            [("HX-Redirect", url)].into_response()
        }
        Err(e) => {
            let note_types: Vec<NoteType> = NoteType::iter().collect();
            RenderHtml(
                key,
                state.tmpl.clone(),
                context!(user => user,
                note_types => note_types,
                sample => sample,
                message => Message {
                    r#type: MessageType::Error,
                    msg: format!("Failed to save note: {}", e),
                }),
            )
            .into_response()
        }
    })
}

async fn show_collection_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, sampleid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    let collection = Collection::fetch(collectionid, &state.dbpool).await?;
    // make sure that this is our sample
    let mut sample = AssignedSample::fetch_one(
        Some(
            FilterBuilder::new(FilterOp::And)
                .push(Arc::new(AssignedSampleFilter::Id(sampleid)))
                .push(Arc::new(AssignedSampleFilter::User(user.id)))
                .push(Arc::new(AssignedSampleFilter::Collection(collectionid)))
                .build(),
        ),
        &state.dbpool,
    )
    .await?;

    sample.fetch_notes(&state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 collection => collection,
                 sample => sample),
    )
    .into_response())
}

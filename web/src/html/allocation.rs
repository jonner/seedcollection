use std::sync::Arc;
use strum::IntoEnumIterator;

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    collection::{self, Allocation, AllocationFilter, Collection, Note, NoteType},
    empty_string_as_none,
    filter::{FilterBuilder, FilterOp},
};
use minijinja::context;
use serde::{Deserialize, Serialize};

use crate::{
    app_url,
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
    Message, MessageType, TemplateKey,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/:sampleid",
            get(show_collection_sample).delete(remove_sample),
        )
        .route(
            "/:sampleid/note/new",
            get(show_add_sample_note).post(add_sample_note),
        )
}

async fn show_collection_sample(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, sampleid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    // make sure that this is our sample
    let mut allocation = Allocation::fetch_one(
        Some(
            FilterBuilder::new(FilterOp::And)
                .push(Arc::new(AllocationFilter::Id(sampleid)))
                .push(Arc::new(AllocationFilter::User(user.id)))
                .push(Arc::new(AllocationFilter::Collection(collectionid)))
                .build(),
        ),
        &state.dbpool,
    )
    .await?;

    allocation.fetch_notes(&state.dbpool).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 allocation => allocation),
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
    // make sure that this is our sample
    let sample = Allocation::fetch_one(
        Some(
            FilterBuilder::new(FilterOp::And)
                .push(Arc::new(AllocationFilter::Id(sampleid)))
                .push(Arc::new(AllocationFilter::User(user.id)))
                .push(Arc::new(AllocationFilter::Collection(collectionid)))
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

async fn show_add_sample_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, sampleid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    let allocation = Allocation::fetch_one(
        Some(
            FilterBuilder::new(FilterOp::And)
                .push(Arc::new(AllocationFilter::Id(sampleid)))
                .push(Arc::new(AllocationFilter::User(user.id)))
                .push(Arc::new(AllocationFilter::Collection(collectionid)))
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
                 note_types => note_types,
                 allocation => allocation),
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
        "DELETE FROM sc_collection_samples AS CS WHERE CS.csid=? AND CS.collectionid IN (SELECT C.collectionid FROM sc_collections AS C WHERE C.userid=?)",
        csid, user.id)
        .execute(&state.dbpool)
        .await?;
    Ok(())
}

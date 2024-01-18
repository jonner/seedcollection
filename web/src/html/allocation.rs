use anyhow::anyhow;
use std::sync::Arc;
use strum::IntoEnumIterator;

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{delete, get},
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::{
    empty_string_as_none,
    filter::{FilterBuilder, FilterOp},
    project::{self, Allocation, AllocationFilter, Note, NoteType, Project},
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
        .route("/:alloc", get(show_allocation).delete(remove_allocation))
        .route("/:alloc/note/:noteid", delete(delete_note))
        .route(
            "/:alloc/note/:noteid/edit",
            get(show_edit_note).put(modify_note),
        )
        .route(
            "/:alloc/note/new",
            get(show_add_allocation_note).post(add_allocation_note),
        )
}

async fn show_allocation(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, allocid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    // make sure that this is our sample
    let mut allocation = Allocation::fetch_one(
        Some(
            FilterBuilder::new(FilterOp::And)
                .push(Arc::new(AllocationFilter::Id(allocid)))
                .push(Arc::new(AllocationFilter::User(user.id)))
                .push(Arc::new(AllocationFilter::Project(collectionid)))
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

async fn add_allocation_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, allocid)): Path<(i64, i64)>,
    Form(params): Form<NoteParams>,
) -> Result<impl IntoResponse, error::Error> {
    // make sure that this is our sample
    let alloc = Allocation::fetch_one(
        Some(
            FilterBuilder::new(FilterOp::And)
                .push(Arc::new(AllocationFilter::Id(allocid)))
                .push(Arc::new(AllocationFilter::User(user.id)))
                .push(Arc::new(AllocationFilter::Project(collectionid)))
                .build(),
        ),
        &state.dbpool,
    )
    .await?;
    let note = Note::new(
        allocid,
        params.date,
        params.notetype,
        params.summary.clone(),
        params.details.as_ref().cloned(),
    );
    Ok(match note.insert(&state.dbpool).await {
        Ok(_) => {
            let url = app_url(&format!("/collection/{}/sample/{}", collectionid, allocid));
            [("HX-Redirect", url)].into_response()
        }
        Err(e) => {
            let note_types: Vec<NoteType> = NoteType::iter().collect();
            RenderHtml(
                key,
                state.tmpl.clone(),
                context!(user => user,
                note_types => note_types,
                allocation => alloc,
                request => params,
                message => Message {
                    r#type: MessageType::Error,
                    msg: format!("Failed to save note: {}", e),
                }),
            )
            .into_response()
        }
    })
}

async fn show_add_allocation_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, allocid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    let allocation = Allocation::fetch_one(
        Some(
            FilterBuilder::new(FilterOp::And)
                .push(Arc::new(AllocationFilter::Id(allocid)))
                .push(Arc::new(AllocationFilter::User(user.id)))
                .push(Arc::new(AllocationFilter::Project(collectionid)))
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

async fn remove_allocation(
    user: SqliteUser,
    State(state): State<AppState>,
    Path((id, csid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    let mut projects =
        Project::fetch_all(Some(Arc::new(project::Filter::Id(id))), &state.dbpool).await?;
    let Some(c) = projects.pop() else {
        return Err(Error::NotFound("That project does not exist".to_string()));
    };
    if c.userid != user.id {
        return Err(Error::NotFound("That project does not exist".to_string()));
    }
    sqlx::query!(
        "DELETE FROM sc_collection_samples AS CS WHERE CS.csid=? AND CS.collectionid IN (SELECT C.collectionid FROM sc_collections AS C WHERE C.userid=?)",
        csid, user.id)
        .execute(&state.dbpool)
        .await?;
    Ok(())
}

async fn delete_note(
    user: SqliteUser,
    TemplateKey(_key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, allocid, noteid)): Path<(i64, i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    // make sure this is a note the user can delete
    let note = Note::fetch(noteid, &state.dbpool).await?;
    let allocation = Allocation::fetch(note.csid, &state.dbpool).await?;
    if note.csid != allocid || allocation.project.id != collectionid {
        return Err(Into::into(anyhow!("Bad request")));
    }
    if allocation.sample.user.id != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete this note".to_string(),
        ));
    }

    note.delete(&state.dbpool).await?;
    Ok(())
}

async fn show_edit_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, allocid, noteid)): Path<(i64, i64, i64)>,
) -> Result<impl IntoResponse, error::Error> {
    // make sure this is a note the user can edit
    let note = Note::fetch(noteid, &state.dbpool).await?;
    let allocation = Allocation::fetch(note.csid, &state.dbpool).await?;
    if note.csid != allocid || allocation.project.id != collectionid {
        return Err(Into::into(anyhow!("Bad request")));
    }
    if allocation.sample.user.id != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete this note".to_string(),
        ));
    }

    let note_types: Vec<NoteType> = NoteType::iter().collect();
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 note => note,
                 note_types => note_types,
                 allocation => allocation),
    )
    .into_response())
}

async fn modify_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((collectionid, allocid, noteid)): Path<(i64, i64, i64)>,
    Form(params): Form<NoteParams>,
) -> Result<impl IntoResponse, error::Error> {
    // make sure this is a note the user can edit
    let mut note = Note::fetch(noteid, &state.dbpool).await?;
    let allocation = Allocation::fetch(note.csid, &state.dbpool).await?;
    if note.csid != allocid || allocation.project.id != collectionid {
        return Err(Into::into(anyhow!("Bad request")));
    }
    if allocation.sample.user.id != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete this note".to_string(),
        ));
    }

    note.date = params.date;
    note.summary = params.summary;
    note.kind = params.notetype;
    note.details = params.details;

    match note.update(&state.dbpool).await {
        Err(e) => {
            let note_types: Vec<NoteType> = NoteType::iter().collect();
            Ok(RenderHtml(
                key,
                state.tmpl.clone(),
                context!(user => user,
                note => note,
                note_types => note_types,
                allocation => allocation,
                message => Message {
                    r#type: MessageType::Error,
                    msg: format!("Failed to update note: {e}"),
                }),
            )
            .into_response())
        }
        Ok(_res) => {
            let url = app_url(&format!("/collection/{collectionid}/sample/{allocid}"));
            Ok([("HX-Redirect", url)].into_response())
        }
    }
}

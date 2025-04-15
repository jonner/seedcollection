use super::error_alert_response;
use crate::{
    TemplateKey,
    auth::SqliteUser,
    error::Error,
    state::AppState,
    util::{FlashMessage, FlashMessageKind, app_url},
};
use anyhow::anyhow;
use axum::{
    Form, Router,
    extract::{Path, State, rejection::FormRejection},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get},
};
use axum_template::RenderHtml;
use libseed::{
    core::{loadable::Loadable, query::filter::and},
    empty_string_as_none,
    project::{self, AllocatedSample, Note, NoteType, Project, allocation},
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use tracing::error;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/{alloc}", get(show_allocation).delete(remove_allocation))
        .route("/{alloc}/note/{noteid}", delete(delete_note))
        .route(
            "/{alloc}/note/{noteid}/edit",
            get(show_edit_note).put(modify_note),
        )
        .route(
            "/{alloc}/note/new",
            get(show_add_allocation_note).post(add_allocation_note),
        )
}

async fn show_allocation(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((projectid, allocid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, Error> {
    // make sure that this is our sample
    let mut allocation = AllocatedSample::load_one(
        Some(
            and()
                .push(allocation::Filter::Id(allocid))
                .push(allocation::Filter::UserId(user.id))
                .push(allocation::Filter::ProjectId(projectid))
                .build(),
        ),
        &state.db,
    )
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => Error::NotFound(format!(
            "Project allocation '{allocid}' was not found in the database"
        )),
        _ => e.into(),
    })?;

    allocation.load_notes(&state.db).await?;
    allocation
        .sample
        .taxon
        .object_mut()?
        .load_germination_info(&state.db)
        .await?;
    let project = Project::load(allocation.projectid, &state.db).await?;
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 project => project,
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
    State(state): State<AppState>,
    Path((projectid, allocid)): Path<(i64, i64)>,
    form: Result<Form<NoteParams>, FormRejection>,
) -> impl IntoResponse {
    let params = match form {
        Ok(Form(params)) => params,
        Err(e) => {
            return error_alert_response(&state, StatusCode::UNPROCESSABLE_ENTITY, e.to_string())
                .into_response();
        }
    };

    // just querying to make sure that this is our sample
    let _alloc = match AllocatedSample::load_one(
        Some(
            and()
                .push(allocation::Filter::Id(allocid))
                .push(allocation::Filter::UserId(user.id))
                .push(allocation::Filter::ProjectId(projectid))
                .build(),
        ),
        &state.db,
    )
    .await
    {
        Ok(alloc) => alloc,
        Err(e) => {
            error!("Failed to fetch allocation: {}", e);
            match e {
                sqlx::Error::RowNotFound => {
                    return error_alert_response(
                        &state,
                        StatusCode::NOT_FOUND,
                        format!("Allocation {allocid} not found for project {projectid}"),
                    )
                    .into_response();
                }
                _ => {
                    return error_alert_response(
                        &state,
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to fetch allocation".to_string(),
                    )
                    .into_response();
                }
            };
        }
    };

    if params.summary.is_empty() {
        return error_alert_response(
            &state,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Summary cannot be empty".to_string(),
        )
        .into_response();
    }

    let mut note = Note::new(
        allocid,
        params.date,
        params.notetype,
        params.summary.clone(),
        params.details.as_ref().cloned(),
    );
    match note.insert(&state.db).await {
        Ok(_) => {
            let url = app_url(&format!("/project/{}/sample/{}", projectid, allocid));
            [("HX-Redirect", url)].into_response()
        }
        Err(e) => {
            error!("Failed to save note: {}", e);
            error_alert_response(
                &state,
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to save note".to_string(),
            )
            .into_response()
        }
    }
}

async fn show_add_allocation_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((projectid, allocid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, Error> {
    let allocation = AllocatedSample::load_one(
        Some(
            and()
                .push(allocation::Filter::Id(allocid))
                .push(allocation::Filter::UserId(user.id))
                .push(allocation::Filter::ProjectId(projectid))
                .build(),
        ),
        &state.db,
    )
    .await?;
    let project = Project::load(projectid, &state.db).await?;
    let note_types: Vec<NoteType> = NoteType::iter().collect();
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 note_types => note_types,
                 project => project,
                 allocation => allocation),
    )
    .into_response())
}

async fn remove_allocation(
    user: SqliteUser,
    State(state): State<AppState>,
    Path((id, psid)): Path<(i64, i64)>,
) -> Result<impl IntoResponse, Error> {
    let mut projects =
        Project::load_all(Some(project::Filter::Id(id).into()), None, None, &state.db).await?;
    let Some(c) = projects.pop() else {
        return Err(Error::NotFound("That project does not exist".to_string()));
    };
    if c.userid != user.id {
        return Err(Error::NotFound("That project does not exist".to_string()));
    }
    sqlx::query!(
        "DELETE FROM sc_project_samples AS PS WHERE PS.psid=? AND PS.projectid IN (SELECT P.projectid FROM sc_projects AS P WHERE P.userid=?)",
        psid, user.id)
        .execute(state.db.pool())
        .await?;
    Ok(())
}

async fn delete_note(
    user: SqliteUser,
    TemplateKey(_key): TemplateKey,
    State(state): State<AppState>,
    Path((projectid, allocid, noteid)): Path<(i64, i64, i64)>,
) -> Result<impl IntoResponse, Error> {
    // make sure this is a note the user can delete
    let mut note = Note::load(noteid, &state.db).await?;
    let allocation = AllocatedSample::load(note.psid, &state.db).await?;
    if note.psid != allocid || allocation.projectid != projectid {
        return Err(Into::into(anyhow!("Bad request")));
    }
    if allocation.sample.user.id() != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete this note".to_string(),
        ));
    }

    note.delete(&state.db).await?;
    Ok(())
}

async fn show_edit_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((projectid, allocid, noteid)): Path<(i64, i64, i64)>,
) -> Result<impl IntoResponse, Error> {
    // make sure this is a note the user can edit
    let note = Note::load(noteid, &state.db).await.map_err(|e| match e {
        libseed::Error::DatabaseError(sqlx::Error::RowNotFound) => {
            Error::NotFound(format!("Unable to find note '{noteid}'"))
        }
        _ => e.into(),
    })?;
    let allocation = AllocatedSample::load(note.psid, &state.db).await?;
    if note.psid != allocid || allocation.projectid != projectid {
        return Err(Into::into(anyhow!("Bad request")));
    }
    if allocation.sample.user.id() != user.id {
        return Err(Error::Unauthorized(
            "User has no permissions for this note".to_string(),
        ));
    }
    let project = Project::load(projectid, &state.db).await?;

    let note_types: Vec<NoteType> = NoteType::iter().collect();
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user,
                 note => note,
                 note_types => note_types,
                 project => project,
                 allocation => allocation),
    )
    .into_response())
}

async fn modify_note(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((projectid, allocid, noteid)): Path<(i64, i64, i64)>,
    Form(params): Form<NoteParams>,
) -> Result<impl IntoResponse, Error> {
    // make sure this is a note the user can edit
    let mut note = Note::load(noteid, &state.db).await?;
    let allocation = AllocatedSample::load(note.psid, &state.db).await?;
    if note.psid != allocid || allocation.projectid != projectid {
        return Err(Into::into(anyhow!("Bad request")));
    }
    if allocation.sample.user.id() != user.id {
        return Err(Error::Unauthorized(
            "No permission to delete this note".to_string(),
        ));
    }
    let project = Project::load(projectid, &state.db).await?;

    note.date = params.date;
    note.summary = params.summary;
    note.kind = params.notetype;
    note.details = params.details;

    match note.update(&state.db).await {
        Err(e) => {
            let note_types: Vec<NoteType> = NoteType::iter().collect();
            Ok(RenderHtml(
                key,
                state.tmpl.clone(),
                context!(user => user,
                note => note,
                note_types => note_types,
                allocation => allocation,
                project => project,
                message => FlashMessage {
                    kind: FlashMessageKind::Error,
                    msg: format!("Failed to update note: {e}"),
                }),
            )
            .into_response())
        }
        Ok(_res) => {
            let url = app_url(&format!("/project/{projectid}/sample/{allocid}"));
            Ok([("HX-Redirect", url)].into_response())
        }
    }
}

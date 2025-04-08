use std::num::NonZero;

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
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use axum_template::RenderHtml;
use libseed::{
    core::loadable::Loadable,
    project::{self, Project},
    sample::{self, Sample, SampleStats},
    source::{self, Source},
    user::UserStatus,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use tracing::warn;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(show_profile).put(update_profile))
        .route("/me/edit", get(show_edit_profile))
        .route("/me/reverify", post(resend_verification))
        .route("/me/prefs", get(show_prefs).put(update_prefs))
}

#[derive(Serialize)]
struct UserStats {
    samples: SampleStats,
    nsources: u64,
    nprojects: u64,
}

async fn show_profile(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let stats = UserStats {
        samples: Sample::stats(Some(sample::Filter::UserId(user.id).into()), &state.db).await?,
        nprojects: Project::count(Some(project::Filter::User(user.id).into()), &state.db).await?,
        nsources: Source::count(Some(source::Filter::UserId(user.id).into()), &state.db).await?,
    };
    let sources: Vec<Source> = Source::load_all_user(user.id, None, &state.db)
        .await?
        .into_iter()
        .filter(|src| src.latitude.is_some() && src.longitude.is_some())
        .collect();
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user, userstats => stats, sources => sources),
    ))
}

async fn show_edit_profile(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => user)))
}

#[derive(Deserialize)]
struct ProfileParams {
    email: String,
    displayname: String,
    profile: String,
}

async fn update_profile(
    mut user: SqliteUser,
    State(state): State<AppState>,
    Form(params): Form<ProfileParams>,
) -> Result<impl IntoResponse, Error> {
    let mut need_reverify = false;
    if params.email.is_empty() {
        return Err(anyhow!("email cannot be empty").into());
    }
    let newemail = params.email.trim();
    if newemail != user.email.trim() {
        user.status = UserStatus::Unverified;
        user.email = newemail.to_string();
        need_reverify = true;
    }
    user.display_name = match params.displayname.trim() {
        "" => None,
        s => Some(s.to_string()),
    };
    user.profile = match params.profile.trim() {
        "" => None,
        s => Some(s.to_string()),
    };
    user.update(&state.db).await?;

    if need_reverify {
        state
            .send_verification(user.generate_verification_request(&state.db).await?)
            .await?;
    }

    Ok([("HX-Redirect", app_url("/user/me"))])
}

async fn resend_verification(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let uv = user.generate_verification_request(&state.db).await?;
    let message = match state.send_verification(uv).await {
        Ok(_) => FlashMessage {
            kind: FlashMessageKind::Success,
            msg: "Sent verification email".to_string(),
        },
        Err(e) => {
            warn!("Failed to send verification email: {e:?}");
            FlashMessage {
                kind: FlashMessageKind::Error,
                msg: "Failed to send verification email".to_string(),
            }
        }
    };

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(message => message),
    ))
}

async fn show_prefs(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), context!(user => user)))
}

#[derive(Deserialize)]
struct PrefsParams {
    pagesize: NonZero<u32>,
}

async fn update_prefs(
    mut user: SqliteUser,
    State(state): State<AppState>,
    TemplateKey(key): TemplateKey,
    Form(params): Form<PrefsParams>,
) -> Result<impl IntoResponse, Error> {
    let prefs = user.preferences_mut(&state.db).await?;
    prefs.pagesize = params.pagesize;
    match prefs.update(&state.db).await {
        Ok(_) => Ok([("HX-Redirect", app_url("/user/me"))].into_response()),
        Err(e) => Ok((StatusCode::INTERNAL_SERVER_ERROR, RenderHtml(
            key,
            state.tmpl.clone(),
            context!(message => FlashMessage { kind: FlashMessageKind::Error, msg: e.to_string()}),
        ))
        .into_response()),
    }
}

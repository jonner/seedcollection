use crate::{auth::SqliteUser, error::Error, state::AppState, TemplateKey};
use anyhow::anyhow;
use axum::{extract::State, response::IntoResponse, routing::get, Form, Router};
use axum_template::RenderHtml;
use libseed::{
    project::{self, Project},
    sample::{self, Sample},
    source::{self, Source},
    user::UserStatus,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(show_profile).put(update_profile))
        .route("/me/edit", get(show_edit_profile))
}

#[derive(Serialize)]
struct UserStats {
    nsamples: i64,
    nsources: i64,
    nprojects: i64,
}

async fn show_profile(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    let stats = UserStats {
        nsamples: Sample::count(Some(Arc::new(sample::Filter::User(user.id))), &state.dbpool)
            .await?,
        nprojects: Project::count(
            Some(Arc::new(project::Filter::User(user.id))),
            &state.dbpool,
        )
        .await?,
        nsources: Source::count(Some(Arc::new(source::Filter::User(user.id))), &state.dbpool)
            .await?,
    };
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => user, userstats => stats),
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
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Form(params): Form<ProfileParams>,
) -> Result<impl IntoResponse, Error> {
    if params.email.is_empty() {
        return Err(anyhow!("email cannot be empty").into());
    }
    let newemail = params.email.trim();
    if newemail != user.email.trim() {
        user.status = UserStatus::Unverified;
        user.email = newemail.to_string();
        todo!("send out a new verification email");
    }
    user.display_name = match params.displayname.trim() {
        s if s.is_empty() => None,
        s => Some(s.to_string()),
    };
    user.profile = match params.profile.trim() {
        s if s.is_empty() => None,
        s => Some(s.to_string()),
    };
    user.update(&state.dbpool).await?;
    Ok(())
}

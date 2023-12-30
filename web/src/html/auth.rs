use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use minijinja::context;
use serde::Deserialize;

use crate::{
    auth::{AuthSession, Credentials},
    error,
    state::AppState,
    CustomKey,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(show_login).post(do_login))
        .route("/logout", get(logout))
        .route("/register", get(show_register).post(register_user))
}

async fn register_user(
    auth: AuthSession,
    Form(creds): Form<Credentials>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(auth
        .backend
        .register(creds.username, creds.password)
        .await?)
}

async fn show_register(
    CustomKey(key): CustomKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), ()))
}

#[derive(Debug, Deserialize)]
pub struct NextUrl {
    next: Option<String>,
}

async fn show_login(
    CustomKey(key): CustomKey,
    auth: AuthSession,
    State(state): State<AppState>,
    Query(NextUrl { next }): Query<NextUrl>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user, next => next),
    ))
}

async fn do_login(
    CustomKey(key): CustomKey,
    mut auth: AuthSession,
    State(state): State<AppState>,
    Form(creds): Form<Credentials>,
) -> Result<impl IntoResponse, error::Error> {
    match auth.authenticate(creds.clone()).await? {
        Some(user) => {
            auth.login(&user).await?;
            let msg = format!(
                r#"<div class="alert alert-success">Logged in as {}</div>"#,
                user.username
            );
            if let Some(next) = creds.next {
                Ok(([("HX-Redirect", next)], msg).into_response())
            } else {
                Ok(msg.into_response())
            }
        }
        None => Ok(RenderHtml(
            key,
            state.tmpl.clone(),
            context!(message => "Invalid credentials"),
        )
        .into_response()),
    }
}

async fn logout(mut auth: AuthSession) -> impl IntoResponse {
    match auth.logout().await {
        Ok(_) => Redirect::to("login").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

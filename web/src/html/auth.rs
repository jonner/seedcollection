use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::get,
    Form, Router,
};
use axum_template::RenderHtml;
use libseed::empty_string_as_none;
use minijinja::context;
use serde::Deserialize;
use tracing::debug;

use crate::{
    app_url,
    auth::{AuthSession, Credentials},
    error,
    state::AppState,
    Message, MessageType, TemplateKey,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(show_login).post(do_login))
        .route("/logout", get(logout))
}

#[derive(Clone, Deserialize)]
pub struct RegisterParams {
    pub username: String,
    pub email: String,
    pub password: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub next: Option<String>,
}

#[allow(dead_code)]
async fn register_user(
    auth: AuthSession,
    Form(params): Form<RegisterParams>,
) -> Result<impl IntoResponse, error::Error> {
    auth.backend
        .register(params.username, params.email, params.password)
        .await
}

#[allow(dead_code)]
async fn show_register(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl.clone(), ()))
}

#[derive(Debug, Deserialize)]
pub struct NextUrl {
    next: Option<String>,
}

async fn show_login(
    TemplateKey(key): TemplateKey,
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
    TemplateKey(key): TemplateKey,
    mut auth: AuthSession,
    State(state): State<AppState>,
    Form(creds): Form<Credentials>,
) -> impl IntoResponse {
    let res = match auth.authenticate(creds.clone()).await {
        Ok(authenticated) => match authenticated {
            Some(user) => match auth.login(&user).await {
                Ok(()) => Ok((
                    [(
                        "HX-Redirect",
                        creds.next.as_ref().cloned().unwrap_or(app_url("/")),
                    )],
                    "",
                )
                    .into_response()),
                Err(e) => Err(format!("Failed to log in: {}", e)),
            },
            None => Err(format!("Failed to find a user '{}'", creds.username)),
        },
        Err(e) => Err(format!("Failed to authenticate: {}", e)),
    };
    match res {
        Ok(resp) => resp,
        Err(msg) => {
            debug!(msg);
            RenderHtml(
                key,
                state.tmpl.clone(),
                context!(message => Message {
                   r#type: MessageType::Error,
                   msg: "Login failed".to_string(),
               },
               username => creds.username,
               next => creds.next),
            )
            .into_response()
        }
    }
}

async fn logout(mut auth: AuthSession) -> impl IntoResponse {
    match auth.logout().await {
        Ok(_) => Redirect::to("login").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

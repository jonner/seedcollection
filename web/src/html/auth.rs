use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::get,
    Form, Router,
};
use axum_login::AuthSession;
use axum_template::RenderHtml;
use minijinja::context;

use crate::{
    auth::{Credentials, SqliteAuthBackend},
    error,
    state::SharedState,
    CustomKey,
};

pub fn router() -> Router<SharedState> {
    Router::new()
        .route("/login", get(show_login).post(do_login))
        .route("/logout", get(logout))
        .route("/register", get(show_register).post(register_user))
}

async fn register_user(
    auth_session: AuthSession<SqliteAuthBackend>,
    Form(creds): Form<Credentials>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(auth_session
        .backend
        .register(creds.username, creds.password)
        .await?)
}

async fn show_register(
    CustomKey(key): CustomKey,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(key, state.tmpl, ()))
}

async fn show_login(
    CustomKey(key): CustomKey,
    auth_session: AuthSession<SqliteAuthBackend>,
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, error::Error> {
    Ok(RenderHtml(
        key,
        state.tmpl,
        context!(user => auth_session.user),
    ))
}

async fn do_login(
    CustomKey(key): CustomKey,
    mut auth_session: AuthSession<SqliteAuthBackend>,
    State(state): State<SharedState>,
    Form(creds): Form<Credentials>,
) -> Result<impl IntoResponse, error::Error> {
    match auth_session.authenticate(creds.clone()).await? {
        Some(user) => {
            auth_session.login(&user).await?;
            Ok(format!("Logged in as {}", user.username).into_response())
        }
        None => Ok(
            RenderHtml(key, state.tmpl, context!(message => "Invalid credentials")).into_response(),
        ),
    }
}

async fn logout(mut auth_session: AuthSession<SqliteAuthBackend>) -> impl IntoResponse {
    match auth_session.logout().await {
        Ok(_) => Redirect::to("login").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

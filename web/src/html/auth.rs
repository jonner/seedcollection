use std::sync::Arc;

use crate::{
    TemplateKey,
    auth::{AuthSession, Credentials},
    error::Error,
    state::{AppState, SharedState},
    util::{
        FlashMessage,
        extract::{Form, Query},
    },
};
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use libseed::{
    core::{error::VerificationError, loadable::Loadable},
    user::{User, UserStatus, verification::UserVerification},
};
use minijinja::context;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use tracing::debug;

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(show_login).post(do_login))
        .route("/logout", post(logout))
        .route(
            "/verify/{userid}/{key}",
            get(show_verification).post(verify_user),
        )
        .route("/register", get(show_register).post(register_user))
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RegisterParams {
    pub(crate) username: String,
    pub(crate) email: String,
    pub(crate) password: SecretString,
    pub(crate) passwordconfirm: SecretString,
}

#[derive(thiserror::Error, Debug, Default)]
#[error("Registration failure: {}", issues.join(", "))]
pub struct RegistrationValidationError {
    pub issues: Vec<String>,
}

impl RegisterParams {
    pub fn validate(&self) -> Result<(), RegistrationValidationError> {
        let mut r = RegistrationValidationError::default();
        const PASSWORD_MIN_LENGTH: u16 = 8;
        if let Err(e) = User::validate_username(&self.username) {
            r.issues.push(e.to_string())
        }
        if self.email.is_empty() {
            r.issues.push("Email address is not valid".to_string())
        }
        if self.password.expose_secret().len() < PASSWORD_MIN_LENGTH as usize {
            r.issues.push(format!(
                "Password must be at least {PASSWORD_MIN_LENGTH} characters long"
            ))
        } else if self.password.expose_secret() != self.passwordconfirm.expose_secret() {
            r.issues.push("Passwords don't match".to_string())
        }
        match r.issues.is_empty() {
            true => Ok(()),
            false => Err(r),
        }
    }
}

async fn register_user(
    State(app): State<AppState>,
    Form(params): Form<RegisterParams>,
) -> Result<impl IntoResponse, Error> {
    if !app.config.user_registration_enabled {
        return Err(Error::UserRegistrationDisabled);
    }
    params.validate()?;
    let password_hash = User::hash_password(params.password.expose_secret())?;
    let mut user = User::new(
        params.username,
        params.email,
        password_hash,
        UserStatus::Unverified,
        None,
        None,
        None,
    );
    user.insert(&app.db).await?;
    let uv = user.generate_verification_request(&app.db).await?;
    app.send_verification(uv).await?;
    Ok([("HX-redirect", app.path("/auth/login"))])
}

async fn show_register(
    TemplateKey(key): TemplateKey,
    State(app): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    if !app.config.user_registration_enabled {
        return Err(Error::UserRegistrationDisabled);
    }
    Ok(app.render_template(key, ()))
}

#[derive(Debug, Deserialize)]
pub(crate) struct NextUrl {
    next: Option<String>,
}

async fn show_login(
    TemplateKey(key): TemplateKey,
    auth: AuthSession,
    State(app): State<AppState>,
    Query(NextUrl { next }): Query<NextUrl>,
) -> Result<impl IntoResponse, Error> {
    Ok(app.render_template(key, context!(user => auth.user, next => next)))
}

async fn do_login(
    State(app): State<AppState>,
    mut auth: AuthSession,
    Form(creds): Form<Credentials>,
) -> Result<impl IntoResponse, Error> {
    let user = auth
        .authenticate(creds.clone())
        .await?
        .ok_or_else(|| Error::UserNotFound(creds.username))?;
    debug!("Authenticated user {}", user.username);
    auth.login(&user).await?;
    debug!("Logged in as {}", user.username);
    Ok((
        [(
            "HX-Redirect",
            creds.next.as_ref().cloned().unwrap_or(app.path("/")),
        )],
        "",
    ))
}

async fn logout(mut auth: AuthSession) -> impl IntoResponse {
    match auth.logout().await {
        Ok(_) => Redirect::to("login").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn verification_error_message(err: VerificationError, app: Arc<SharedState>) -> FlashMessage {
    let profile_url = app.path("/user/me");
    match err {
        VerificationError::Expired => FlashMessage::Error(format!(
            "This verification code has expired. Please visit your <a href='{profile_url}'>user profile</a> to request a new verification code to be emailed to you."
        )),
        VerificationError::AlreadyVerified => {
            FlashMessage::Info("This email address has already been verified.".into())
        }
        VerificationError::InternalError(_)
        | VerificationError::MultipleKeysFound
        | VerificationError::KeyNotFound => FlashMessage::Warning(format!(
            "The verification code you provided could not be found. Check your verification email and make sure that the link you clicked was not corrupted in some way. Visit your <a href='{profile_url}'>user profile</a> to request a new verification code to be emailed to you. "
        )),
    }
}

async fn show_verification(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(app): State<AppState>,
    Path((userid, vkey)): Path<(i64, String)>,
) -> Result<impl IntoResponse, Error> {
    let message = UserVerification::find(userid, &vkey, &app.db)
        .await
        .map_or_else(|err| verification_error_message(err, app.clone()), |_uv| {
            FlashMessage::Warning(
                "Verification of your email address is required in order to perform some actions on this website. Click below to verify your email address."
                    .into(),
            )
        });
    Ok(app.render_template(key, context!(message => message, user => auth.user)))
}

async fn verify_user(
    TemplateKey(key): TemplateKey,
    State(app): State<AppState>,
    Path((userid, vkey)): Path<(i64, String)>,
) -> Result<impl IntoResponse, Error> {
    let mut user = User::load(userid, &app.db).await?;
    let res = user.verify(&vkey, &app.db).await;
    let message = match res {
        Ok(_) => FlashMessage::Success("You have successfully verified the account".into()),
        Err(e) => match e {
            libseed::Error::UserVerification(e) => verification_error_message(e, app.clone()),
            _e => FlashMessage::Error("Failed to verify user".into()),
        },
    };

    Ok(app.render_template(key, context!(message => message)))
}

#[cfg(test)]
mod test {
    use crate::test_app;

    use super::*;
    use axum::{body::Body, http::Request};
    use sqlx::{Pool, Sqlite};
    use test_log::test;
    use tower::Service;

    // expires in an hour
    const KEY: &str = "aRbitrarykeyvaluej0asvdo6q134fn2B0Xuw3r42i1o";
    // user id associated with the valid key
    const USERID: <User as Loadable>::Id = 1;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../../db/fixtures",
            scripts("users", "sources", "taxa", "user-verifications")
        )
    ))]
    async fn test_auth_verify_user(pool: Pool<Sqlite>) {
        let _ = tracing_subscriber::fmt::try_init();
        let (mut app, state) = test_app(pool).await.expect("failed to create test app").0;
        let user = User::load(USERID, &state.db)
            .await
            .expect("Failed to find user");
        assert_eq!(user.status, UserStatus::Unverified);
        let uv = UserVerification::find(USERID, KEY, &state.db)
            .await
            .expect("Failed to find UserVerification object");
        debug!(?uv);
        assert!(!uv.confirmed);

        // then try to add a note
        let req = Request::builder()
            .uri(state.path(format!("/auth/verify/{USERID}/{KEY}",).as_str()))
            .method("POST")
            .body(Body::empty())
            .expect("Failed to build request");
        let response = app
            .as_service()
            .call(req)
            .await
            .expect("Failed to execute request");
        assert_eq!(response.status(), StatusCode::OK);

        let uv = UserVerification::load(uv.id, &state.db)
            .await
            .expect("Failed to find UserVerification object");
        assert!(uv.confirmed);
        let user = User::load(USERID, &state.db)
            .await
            .expect("Failed to find user verification request");
        assert_eq!(user.status, UserStatus::Verified);
    }
}

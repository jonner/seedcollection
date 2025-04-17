use crate::{
    TemplateKey,
    auth::{AuthSession, Credentials},
    error::Error,
    html::flash_message,
    state::AppState,
    util::{
        FlashMessage, FlashMessageKind, app_url,
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
use axum_template::RenderHtml;
use libseed::{
    core::{error::VerificationError, loadable::Loadable},
    user::{User, UserStatus, verification::UserVerification},
};
use minijinja::context;
use serde::Deserialize;
use tracing::error;

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

#[derive(Clone, Deserialize)]
pub(crate) struct RegisterParams {
    pub(crate) username: String,
    pub(crate) email: String,
    pub(crate) password: String,
    pub(crate) passwordconfirm: String,
}

impl RegisterParams {
    pub fn validate(&self) -> Result<(), Vec<FlashMessage>> {
        let mut flash_messages: Vec<FlashMessage> = Vec::default();
        const PASSWORD_MIN_LENGTH: u16 = 8;
        if let Err(e) = User::validate_username(&self.username) {
            flash_messages.push(FlashMessage {
                kind: FlashMessageKind::Error,
                msg: e.to_string(),
            })
        }
        if self.email.is_empty() {
            flash_messages.push(FlashMessage {
                kind: FlashMessageKind::Error,
                msg: "Email address is not valid".to_string(),
            })
        }
        if self.password.len() < PASSWORD_MIN_LENGTH as usize {
            flash_messages.push(FlashMessage {
                kind: FlashMessageKind::Error,
                msg: format!("Password must be at least {PASSWORD_MIN_LENGTH} characters long"),
            })
        } else if self.password != self.passwordconfirm {
            flash_messages.push(FlashMessage {
                kind: FlashMessageKind::Error,
                msg: "Passwords don't match".to_string(),
            })
        }
        match flash_messages.is_empty() {
            true => Ok(()),
            false => Err(flash_messages),
        }
    }
}

async fn register_user(
    State(state): State<AppState>,
    TemplateKey(key): TemplateKey,
    Form(params): Form<RegisterParams>,
) -> Result<impl IntoResponse, Error> {
    if !state.config.user_registration_enabled {
        return Err(Error::UserRegistrationDisabled);
    }
    match params.validate() {
        Ok(_) => {
            let password_hash = User::hash_password(&params.password)?;
            let mut user = User::new(
                params.username,
                params.email,
                password_hash,
                UserStatus::Unverified,
                None,
                None,
                None,
            );
            user.insert(&state.db).await?;
            let uv = user.generate_verification_request(&state.db).await?;
            state.send_verification(uv).await?;
            Ok([("HX-redirect", app_url("/auth/login"))].into_response())
        }
        Err(messages) => Ok(RenderHtml(
            key,
            state.tmpl.clone(),
            context! {
                username => params.username,
                email_address => params.email,
                messages => messages,
            },
        )
        .into_response()),
    }
}

async fn show_register(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, Error> {
    if !state.config.user_registration_enabled {
        return Err(Error::UserRegistrationDisabled);
    }
    Ok(RenderHtml(key, state.tmpl.clone(), ()))
}

#[derive(Debug, Deserialize)]
pub(crate) struct NextUrl {
    next: Option<String>,
}

async fn show_login(
    TemplateKey(key): TemplateKey,
    auth: AuthSession,
    State(state): State<AppState>,
    Query(NextUrl { next }): Query<NextUrl>,
) -> Result<impl IntoResponse, Error> {
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(user => auth.user, next => next),
    ))
}

fn login_failure_response<E: std::fmt::Debug>(
    state: &AppState,
    context: &str,
    err: Option<E>,
) -> impl IntoResponse {
    error!("{context}: {err:?}");

    (
        StatusCode::UNAUTHORIZED,
        flash_message(
            state.clone(),
            FlashMessageKind::Error,
            "Incorrect username or password. Please double-check and try again.".to_string(),
        )
        .into_response(),
    )
}

async fn do_login(
    mut auth: AuthSession,
    State(state): State<AppState>,
    Form(creds): Form<Credentials>,
) -> impl IntoResponse {
    match auth.authenticate(creds.clone()).await {
        Ok(authenticated) => match authenticated {
            Some(user) => match auth.login(&user).await {
                Ok(()) => (
                    [(
                        "HX-Redirect",
                        creds.next.as_ref().cloned().unwrap_or(app_url("/")),
                    )],
                    "",
                )
                    .into_response(),
                Err(e) => {
                    login_failure_response(&state, "Failed to login", Some(e)).into_response()
                }
            },
            None => login_failure_response::<&str>(
                &state,
                &format!("Failed to find a user '{}'", creds.username),
                None,
            )
            .into_response(),
        },
        Err(e) => {
            login_failure_response(&state, "Failed to authenticate", Some(&e)).into_response()
        }
    }
}

async fn logout(mut auth: AuthSession) -> impl IntoResponse {
    match auth.logout().await {
        Ok(_) => Redirect::to("login").into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn verification_error_message(err: VerificationError) -> FlashMessage {
    let profile_url = app_url("/user/me");

    match err {
        VerificationError::Expired => FlashMessage {
            kind: FlashMessageKind::Error,
            msg: format!(
                "This verification code has expired. Please visit your <a href='{profile_url}'>user profile</a> to request a new verification code to be emailed to you."
            ),
        },
        VerificationError::AlreadyVerified => FlashMessage {
            kind: FlashMessageKind::Info,
            msg: "This email address has already been verified.".into(),
        },
        VerificationError::InternalError(_)
        | VerificationError::MultipleKeysFound
        | VerificationError::KeyNotFound => FlashMessage {
            kind: FlashMessageKind::Warning,
            msg: format!(
                "The verification code you provided could not be found. Check your verification email and make sure that the link you clicked was not corrupted in some way. Visit your <a href='{profile_url}'>user profile</a> to request a new verification code to be emailed to you. "
            ),
        },
    }
}

async fn show_verification(
    auth: AuthSession,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((userid, vkey)): Path<(i64, String)>,
) -> Result<impl IntoResponse, Error> {
    let message = UserVerification::find(userid, &vkey, &state.db)
        .await
        .map_or_else(verification_error_message, |_uv| FlashMessage {
            kind: FlashMessageKind::Warning,
            msg: "Verification of your email address is required in order to perform
            some actions on this website. Click below to verify your email
            address."
                .into(),
        });
    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(message => message, user => auth.user),
    ))
}

async fn verify_user(
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
    Path((userid, vkey)): Path<(i64, String)>,
) -> Result<impl IntoResponse, Error> {
    let res = UserVerification::find(userid, &vkey, &state.db).await;
    let message = match res {
        Ok(mut uv) => match uv.verify(&state.db).await {
            Ok(_) => FlashMessage {
                kind: FlashMessageKind::Success,
                msg: "You have successfully verified your account".into(),
            },
            Err(_e) => FlashMessage {
                kind: FlashMessageKind::Error,
                msg: "Failed to verify user".into(),
            },
        },
        Err(e) => verification_error_message(e),
    };

    Ok(RenderHtml(
        key,
        state.tmpl.clone(),
        context!(message => message),
    ))
}

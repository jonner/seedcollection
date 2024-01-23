use crate::{
    app_url,
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
    Message, MessageType, TemplateKey,
};
use anyhow::{anyhow, Context};
use axum::{
    extract::State,
    response::IntoResponse,
    routing::{get, post},
    Form, Router,
};
use axum_template::{RenderHtml, TemplateEngine};
use lettre::{
    message::{header::ContentType, Mailbox},
    AsyncFileTransport, AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
};
use libseed::{
    project::{self, Project},
    sample::{self, Sample},
    source::{self, Source},
    user::UserStatus,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::warn;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(show_profile).put(update_profile))
        .route("/me/edit", get(show_edit_profile))
        .route("/me/reverify", post(resend_verification))
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
    State(state): State<AppState>,
    Form(params): Form<ProfileParams>,
) -> Result<impl IntoResponse, error::Error> {
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
        s if s.is_empty() => None,
        s => Some(s.to_string()),
    };
    user.profile = match params.profile.trim() {
        s if s.is_empty() => None,
        s => Some(s.to_string()),
    };
    user.update(&state.dbpool).await?;

    if need_reverify {
        send_verification(user, &state).await?;
    }

    Ok([("HX-Redirect", app_url("/user/me"))])
}

async fn send_verification(user: SqliteUser, state: &AppState) -> Result<(), error::Error> {
    let uvkey = user.new_verification_code(&state.dbpool).await?;
    // FIXME: figure out how to do the host/port stuff properly. Right now this will send a link to
    // host 0.0.0.0 if that's what we configured the server to listen on...
    let mut verification_url = "https://".to_string();
    verification_url.push_str(&state.config.listen.host);
    if state.config.listen.https_port != 443 {
        verification_url.push_str(&format!(":{}", state.config.listen.https_port));
    }
    verification_url.push_str(&app_url(&format!("/auth/verify/{uvkey}")));
    let emailbody = state
        .tmpl
        .render(
            "verification-email",
            context!(user => user,
                     verification_url => verification_url),
        )
        .with_context(|| "Failed to render email")?;
    let email = lettre::Message::builder()
        .from(
            "NOBODY <jonathon@quotidian.org>"
                .parse()
                .with_context(|| "failed to parse sender address")?,
        )
        .to(Mailbox::new(
            user.display_name.clone(),
            user.email
                .parse()
                .with_context(|| "Failed to parse recipient address")?,
        ))
        .subject("Verify your email address")
        .header(ContentType::TEXT_PLAIN)
        .body(emailbody)
        .with_context(|| "Failed to create email message")?;
    match state.config.mail {
        crate::MailSender::FileTransport(ref path) => {
            AsyncFileTransport::<Tokio1Executor>::new(path)
                .send(email)
                .await
                .map_err(anyhow::Error::from)
                .map(|_| ())
        }
        crate::MailSender::LocalSmtpTransport => {
            AsyncSmtpTransport::<Tokio1Executor>::unencrypted_localhost()
                .send(email)
                .await
                .map_err(anyhow::Error::from)
                .map(|_| ())
        }
        crate::MailSender::SmtpTransport(ref cfg) => cfg
            .build()?
            .send(email)
            .await
            .map_err(anyhow::Error::from)
            .map(|_| ()),
    }
    .with_context(|| "Failed to send email")
    .map_err(|e| e.into())
}

async fn resend_verification(
    user: SqliteUser,
    TemplateKey(key): TemplateKey,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, error::Error> {
    let message = match send_verification(user, &state).await {
        Ok(_) => Message {
            r#type: MessageType::Success,
            msg: "Sent verification email".to_string(),
        },
        Err(e) => {
            warn!("Failed to send verification email: {e:?}");
            Message {
                r#type: MessageType::Error,
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

use crate::{
    auth::SqliteUser,
    error::{self, Error},
    state::AppState,
    util::app_url,
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
    sample::{self, Sample, SampleStats},
    source::{self, Source},
    user::UserStatus,
};
use minijinja::context;
use serde::{Deserialize, Serialize};
use tracing::warn;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/me", get(show_profile).put(update_profile))
        .route("/me/edit", get(show_edit_profile))
        .route("/me/reverify", post(resend_verification))
}

#[derive(Serialize)]
struct UserStats {
    samples: SampleStats,
    nsources: i64,
    nprojects: i64,
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
        "" => None,
        s => Some(s.to_string()),
    };
    user.profile = match params.profile.trim() {
        "" => None,
        s => Some(s.to_string()),
    };
    user.update(&state.db).await?;

    if need_reverify {
        send_verification(user, &state).await?;
    }

    Ok([("HX-Redirect", app_url("/user/me"))])
}

fn verification_url(state: &std::sync::Arc<crate::state::SharedState>, vcode: String) -> String {
    // FIXME: figure out how to do the host/port stuff properly. Right now this will send a link to
    // host 0.0.0.0 if that's what we configured the server to listen on...
    let mut url = "https://".to_string();
    url.push_str(&state.config.listen.host);
    if state.config.listen.https_port != 443 {
        url.push_str(&format!(":{}", state.config.listen.https_port));
    }
    url.push_str(&app_url(&format!("/auth/verify/{vcode}")));
    url
}

fn verification_email(
    state: &std::sync::Arc<crate::state::SharedState>,
    vcode: String,
    user: SqliteUser,
) -> Result<lettre::Message, Error> {
    let verification_url = verification_url(state, vcode);
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
    Ok(email)
}

async fn send_verification(user: SqliteUser, state: &AppState) -> Result<(), error::Error> {
    let vcode = user.new_verification_code(&state.db).await?;
    let email = verification_email(state, vcode, user)?;
    match state.config.mail_transport {
        crate::MailTransport::File(ref path) => AsyncFileTransport::<Tokio1Executor>::new(path)
            .send(email)
            .await
            .map_err(anyhow::Error::from)
            .map(|_| ()),
        crate::MailTransport::LocalSmtp => {
            AsyncSmtpTransport::<Tokio1Executor>::unencrypted_localhost()
                .send(email)
                .await
                .map_err(anyhow::Error::from)
                .map(|_| ())
        }
        crate::MailTransport::Smtp(ref cfg) => cfg
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

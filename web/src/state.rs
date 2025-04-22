use crate::{EnvConfig, error::Error, template_engine, util::app_url};
use anyhow::{Context, Result};
use axum_template::{RenderHtml, TemplateEngine, engine::Engine};
use lettre::{
    AsyncFileTransport, AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
    message::{Mailbox, header::ContentType},
};
use libseed::{core::database::Database, user::verification::UserVerification};
use minijinja::context;
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tracing::{debug, trace};

#[derive(Debug)]
pub(crate) struct SharedState {
    pub(crate) db: Database,
    pub(crate) tmpl: Engine<minijinja::Environment<'static>>,
    pub(crate) config: EnvConfig,
    pub(crate) datadir: PathBuf,
}

impl SharedState {
    pub(crate) async fn new(envname: &str, env: EnvConfig, datadir: PathBuf) -> Result<Self> {
        let tmpl_path = datadir.join("templates");
        let template = template_engine(envname, &tmpl_path);
        trace!("Creating shared app state");
        // do a quick sanity check on the mail transport
        debug!(?env.mail_transport,
            "Sanity checking the mail transport",
        );
        match env.mail_transport {
            crate::MailTransport::File(_) => Ok(()),
            crate::MailTransport::LocalSmtp => {
                let t = AsyncSmtpTransport::<Tokio1Executor>::unencrypted_localhost();
                t.test_connection().await.map(|_| ())
            }
            crate::MailTransport::Smtp(ref cfg) => {
                let t = cfg.build()?;
                t.test_connection().await.map(|_| ())
            }
        }
        .with_context(|| "Sanity check of mail transport failed")?;
        Ok(Self {
            db: Database::open(env.database.clone())
                .await
                .with_context(|| format!("Unable to open database {}", &env.database))?,
            tmpl: template,
            config: env,
            datadir,
        })
    }

    async fn create_verification_email(
        &self,
        uv: &mut UserVerification,
    ) -> Result<lettre::Message, Error> {
        // FIXME: figure out how to do the host/port stuff properly. Right now this will send a link to
        // host 0.0.0.0 if that's what we configured the server to listen on...
        let mut verification_url = "https://".to_string();
        verification_url.push_str(&self.config.listen.host);
        if self.config.listen.https_port != 443 {
            verification_url.push_str(&format!(":{}", self.config.listen.https_port));
        }
        verification_url.push_str(&app_url(&format!(
            "/auth/verify/{}/{}",
            uv.user.id(),
            uv.key
        )));
        let user = uv.user.load(&self.db, false).await?;
        let emailbody = self
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

    pub async fn send_verification(&self, mut uv: UserVerification) -> Result<(), Error> {
        let email = self.create_verification_email(&mut uv).await?;
        match self.config.mail_transport {
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
        .with_context(|| "Failed to send verification email")
        .map_err(|e| e.into())
    }

    pub fn render_template<'a, K, S>(
        self: Arc<Self>,
        template_key: K,
        data: S,
    ) -> RenderHtml<K, Engine<minijinja::Environment<'a>>, S>
    where
        S: Serialize,
    {
        RenderHtml(template_key, self.tmpl.clone(), data)
    }

    #[cfg(test)]
    pub(crate) fn test(pool: sqlx::Pool<sqlx::Sqlite>) -> Self {
        let template = template_engine("test", "./templates");
        debug!("Creating test shared app state");
        Self {
            db: Database::from(pool),
            tmpl: template,
            config: EnvConfig {
                listen: crate::ListenConfig {
                    host: "127.0.0.1".to_string(),
                    http_port: 8080,
                    https_port: 8443,
                },
                database: "test-database.sqlite".to_string(),
                mail_transport: crate::MailTransport::File("/tmp/".to_string()),
                user_registration_enabled: false,
            },
            datadir: ".".into(),
        }
    }
}

pub(crate) type AppState = Arc<SharedState>;

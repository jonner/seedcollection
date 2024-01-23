use crate::{db, EnvConfig};
use anyhow::{Context, Result};
use axum_template::engine::Engine;
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::debug;

type TemplateEngine = Engine<minijinja::Environment<'static>>;

pub struct SharedState {
    pub dbpool: SqlitePool,
    pub tmpl: TemplateEngine,
    pub config: EnvConfig,
}

impl SharedState {
    pub async fn new(env: EnvConfig, template: TemplateEngine) -> Result<Self> {
        debug!("Creating shared app state");
        // do a quick sanity check on the mail transport
        debug!("Sanity checking the mail transport '{:?}'", env.mail);
        match env.mail {
            crate::MailSender::FileTransport(_) => Ok(()),
            crate::MailSender::LocalSmtpTransport => {
                let t = AsyncSmtpTransport::<Tokio1Executor>::unencrypted_localhost();
                t.test_connection().await.map(|_| ())
            }
            crate::MailSender::SmtpTransport(ref cfg) => {
                let t = cfg.build()?;
                t.test_connection().await.map(|_| ())
            }
        }
        .with_context(|| "Sanity check of mail transport failed")?;
        Ok(Self {
            dbpool: db::pool(env.database.clone()).await?,
            tmpl: template,
            config: env,
        })
    }
}

pub type AppState = Arc<SharedState>;

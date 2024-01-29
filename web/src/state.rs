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
        debug!(
            "Sanity checking the mail transport '{:?}'",
            env.mail_transport
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
            dbpool: db::pool(env.database.clone()).await?,
            tmpl: template,
            config: env,
        })
    }

    #[cfg(test)]
    pub fn test(pool: sqlx::Pool<sqlx::Sqlite>, template: TemplateEngine) -> Self {
        debug!("Creating test shared app state");
        Self {
            dbpool: pool,
            tmpl: template,
            config: EnvConfig {
                listen: crate::ListenConfig {
                    host: "127.0.0.1".to_string(),
                    http_port: 8080,
                    https_port: 8443,
                },
                database: "test-database.sqlite".to_string(),
                mail_transport: crate::MailTransport::File("/tmp/".to_string()),
            },
        }
    }
}

pub type AppState = Arc<SharedState>;

use crate::{db, template_engine, EnvConfig};
use anyhow::{Context, Result};
use axum_template::engine::Engine;
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use sqlx::SqlitePool;
use std::{path::PathBuf, sync::Arc};
use tracing::{debug, trace};

type TemplateEngine = Engine<minijinja::Environment<'static>>;

#[derive(Debug)]
pub struct SharedState {
    pub dbpool: SqlitePool,
    pub tmpl: TemplateEngine,
    pub config: EnvConfig,
    pub datadir: PathBuf,
}

impl SharedState {
    pub async fn new(envname: &str, env: EnvConfig, datadir: PathBuf) -> Result<Self> {
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
            dbpool: db::pool(env.database.clone())
                .await
                .with_context(|| format!("Unable to open database {}", &env.database))?,
            tmpl: template,
            config: env,
            datadir,
        })
    }

    #[cfg(test)]
    pub fn test(pool: sqlx::Pool<sqlx::Sqlite>) -> Self {
        let template = template_engine("test", "./templates");
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
            datadir: ".".into(),
        }
    }
}

pub type AppState = Arc<SharedState>;

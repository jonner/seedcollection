use crate::db;
use anyhow::Result;
use axum_template::engine::Engine;
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::debug;

type TemplateEngine = Engine<minijinja::Environment<'static>>;

#[derive(Clone)]
pub struct SharedState {
    pub dbpool: SqlitePool,
    pub tmpl: TemplateEngine,
    pub host: String,
    pub port: u16,
}

impl SharedState {
    pub async fn new(
        dbpath: String,
        template: TemplateEngine,
        host: String,
        port: u16,
    ) -> Result<Self> {
        debug!("Creating shared app state");
        Ok(Self {
            dbpool: db::pool(dbpath).await?,
            tmpl: template,
            host,
            port,
        })
    }
}

pub type AppState = Arc<SharedState>;

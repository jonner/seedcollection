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
}

impl SharedState {
    pub async fn new(dbpath: String, template: TemplateEngine) -> Result<Self> {
        debug!("Creating shared app state");
        Ok(Self {
            dbpool: db::pool(dbpath).await?,
            tmpl: template,
        })
    }
}

pub type AppState = Arc<SharedState>;

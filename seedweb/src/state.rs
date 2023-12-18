use crate::db;
use anyhow::Result;
use axum_template::engine::Engine;
use minijinja::Environment;
use sqlx::SqlitePool;

type TemplateEngine = Engine<Environment<'static>>;

#[derive(Clone)]
pub struct SharedState {
    pub dbpool: SqlitePool,
    pub tmpl: TemplateEngine,
}

impl SharedState {
    pub async fn new(dbpath: String, template: TemplateEngine) -> Result<Self> {
        Ok(Self {
            dbpool: db::pool(dbpath).await?,
            tmpl: template,
        })
    }
}

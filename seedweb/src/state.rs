use crate::db;
use anyhow::Result;
use sqlx::SqlitePool;

pub struct SharedState {
    pub dbpool: SqlitePool,
}

impl SharedState {
    pub async fn new(dbpath: String) -> Result<Self> {
        Ok(Self {
            dbpool: db::pool(dbpath).await?,
        })
    }
}

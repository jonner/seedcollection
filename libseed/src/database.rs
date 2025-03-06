use crate::error::Result;
use sqlx::{Pool, Sqlite, SqlitePool};
use std::path::Path;
use tracing::trace;

/// An object that represents a connection to the seed collection database
#[derive(Clone, Debug)]
pub struct Database(Pool<Sqlite>);

impl Database {
    /// Open a connection to the specified database. This will also perform any
    /// necessary sql migrations to ensure that the database is up to date with the
    /// latest schema changes.
    pub async fn open<P: AsRef<Path>>(db: P) -> Result<Self> {
        let dbpool =
            SqlitePool::connect(&format!("sqlite://{}", db.as_ref().to_string_lossy())).await?;
        trace!("Running database migrations");
        sqlx::migrate!("../db/migrations").run(&dbpool).await?;
        Ok(Database(dbpool))
    }

    /// gets a reference to the underlying sqlx connection pool
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.0
    }

    /// This constructor is primarily intended for tests. Use [Database::open()]
    /// instead as that function will perform database schema migration
    /// automatically.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self(pool)
    }
}

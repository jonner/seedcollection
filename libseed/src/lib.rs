//! This is a library that provides objects and functionality to help you manage a native seed
//! collection and keep track of everything inside of a database.

use serde::{Deserialize, Deserializer};
use sqlx::{Pool, Sqlite, SqlitePool};
use std::path::Path;
use std::str::FromStr;
use tracing::trace;

mod error;
pub mod loadable;
pub mod project;
pub mod query;
pub mod sample;
pub mod source;
pub mod taxonomy;
pub mod user;

pub use error::Error;
pub use error::Result;

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

/// A utility function to deserialize an Optional value as None when it is an
/// empty string
pub fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s)
            .map_err(serde::de::Error::custom)
            .map(Some),
    }
}

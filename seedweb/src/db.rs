use anyhow::Result;
use sqlx::SqlitePool;

pub async fn pool() -> Result<SqlitePool> {
    Ok(SqlitePool::connect("sqlite://seedcollection.sqlite").await?)
}

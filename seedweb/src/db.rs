use anyhow::Result;
use sqlx::SqlitePool;

pub async fn pool(db: String) -> Result<SqlitePool> {
    Ok(SqlitePool::connect(&format!("sqlite://{}", db)).await?)
}

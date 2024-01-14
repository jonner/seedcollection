use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};

#[derive(sqlx::FromRow, Serialize, Deserialize)]
pub struct User {
    #[sqlx(rename = "userid")]
    pub id: i64,
    pub username: String,
}

impl User {
    pub async fn fetch_all(pool: &Pool<Sqlite>) -> anyhow::Result<Vec<User>> {
        Ok(
            sqlx::query_as("SELECT id as userid, username FROM sc_users ORDER BY username ASC")
                .fetch_all(pool)
                .await?,
        )
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> anyhow::Result<User> {
        Ok(
            sqlx::query_as("SELECT id as userid, username FROM sc_users WHERE id=?")
                .bind(id)
                .fetch_one(pool)
                .await?,
        )
    }

    // this just creates a placeholder object to hold an ID so that another object (e.g. sample)
    // that contains a Taxon object can still exist without loading the entire taxon from the
    // database
    pub fn new_id_only(id: i64) -> Self {
        User {
            id,
            username: Default::default(),
        }
    }
}

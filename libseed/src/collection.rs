use crate::{
    filter,
    sample::{self, Sample},
};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite};

#[derive(sqlx::FromRow, Deserialize, Serialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<Sample>,
}

impl Collection {
    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> anyhow::Result<Self> {
        Ok(
            sqlx::query_as("SELECT id, name, description FROM sc_collections WHERE id=?")
                .bind(id)
                .fetch_one(pool)
                .await?,
        )
    }

    pub async fn fetch_all(pool: &Pool<Sqlite>) -> anyhow::Result<Vec<Self>> {
        Ok(
            sqlx::query_as("SELECT id, name, description FROM sc_collections")
                .fetch_all(pool)
                .await?,
        )
    }

    pub async fn fetch_samples(&mut self, pool: &Pool<Sqlite>) -> anyhow::Result<()> {
        self.samples = Sample::fetch_all(
            Some(Box::new(sample::Filter::Collection(
                filter::Cmp::Equal,
                self.id,
            ))),
            pool,
        )
        .await?;
        Ok(())
    }
}

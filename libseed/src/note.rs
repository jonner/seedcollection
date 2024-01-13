use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sqlx::{Pool, QueryBuilder, Sqlite};
use strum_macros::{EnumIter, EnumString, FromRepr};
use time::Date;

use crate::filter::{DynFilterPart, FilterPart};

#[derive(
    sqlx::Type, Debug, Copy, Clone, Serialize, Deserialize, EnumString, EnumIter, FromRepr,
)]
pub enum NoteType {
    Preparation = 1,
    Germination = 2,
    Planting = 3,
    Growing = 4,
    Other = 5,
}

#[derive(Clone)]
pub enum FilterField {
    Id(i64),
    CollectionSample(i64),
}

#[derive(sqlx::FromRow, Deserialize, Serialize, Debug)]
pub struct Note {
    pub id: i64,
    pub csid: i64,
    pub date: Date,
    pub kind: NoteType,
    pub summary: String,
    pub details: Option<String>,
}

impl FilterPart for FilterField {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(i) => _ = builder.push(" id=").push_bind(*i),
            Self::CollectionSample(i) => _ = builder.push(" csid=").push_bind(*i),
        }
    }
}

impl Note {
    fn build_query(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder = QueryBuilder::new(
            r#"SELECT id, csid, date, kind, summary, details FROM sc_collection_sample_notes"#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" ORDER BY csid, date");
        builder
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> Result<Note, sqlx::Error> {
        Self::build_query(Some(Arc::new(FilterField::Id(id))))
            .build_query_as()
            .fetch_one(pool)
            .await
    }

    pub async fn fetch_all(
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Note>, sqlx::Error> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_all(pool)
            .await
    }

    pub async fn insert(&self, pool: &Pool<Sqlite>) -> Result<Note, sqlx::Error> {
        sqlx::query_as(
            r#"INSERT INTO sc_collection_sample_notes
            (csid, date, kind, summary, details)
            VALUES (?, ?, ?, ?, ?) RETURNING *"#,
        )
        .bind(self.csid)
        .bind(self.date)
        .bind(self.kind)
        .bind(&self.summary)
        .bind(&self.details)
        .fetch_one(pool)
        .await
    }

    pub async fn update(&self, pool: &Pool<Sqlite>) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE sc_collection_sample_notes
            SET csid=?, date=?, kind=?, summary=?, details=? WHERE id=?"#,
        )
        .bind(self.csid)
        .bind(self.date)
        .bind(self.kind)
        .bind(&self.summary)
        .bind(&self.details)
        .bind(self.id)
        .execute(pool)
        .await
        .map(|_| ())
    }
}

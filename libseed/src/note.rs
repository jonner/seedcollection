use sqlx::{Pool, QueryBuilder, Sqlite};
use time::Date;

use crate::filter::FilterPart;

#[derive(sqlx::Type, Copy, Clone)]
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

#[derive(sqlx::FromRow)]
pub struct Note {
    id: i64,
    csid: i64,
    date: Date,
    kind: NoteType,
    summary: String,
    details: Option<String>,
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
    fn build_query(filter: Option<Box<dyn FilterPart>>) -> QueryBuilder<'static, Sqlite> {
        let mut builder = QueryBuilder::new(
            r#"SELECT id, csid, date, type, summary, details FROM sc_collection_sample_notes"#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" ORDER BY csid, date");
        builder
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> Result<Note, sqlx::Error> {
        Self::build_query(Some(Box::new(FilterField::Id(id))))
            .build_query_as()
            .fetch_one(pool)
            .await
    }

    pub async fn fetch_all(id: i64, pool: &Pool<Sqlite>) -> Result<Vec<Note>, sqlx::Error> {
        Self::build_query(Some(Box::new(FilterField::CollectionSample(id))))
            .build_query_as()
            .fetch_all(pool)
            .await
    }

    pub async fn insert(&self, pool: &Pool<Sqlite>) -> Result<Note, sqlx::Error> {
        sqlx::query_as(
            r#"INSERT INTO sc_collection_sample_notes
            (csid, date, type, summary, details)
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
            SET csid=?, date=?, type=?, summary=?, details=? WHERE id=?"#,
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

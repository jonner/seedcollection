use anyhow::anyhow;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, QueryBuilder, Sqlite};
use std::sync::Arc;
use strum_macros::{EnumIter, EnumString, FromRepr};
use time::Date;

use crate::{
    filter::{DynFilterPart, FilterPart},
    loadable::Loadable,
};

#[derive(
    sqlx::Type, Debug, Copy, Clone, Serialize, Deserialize, EnumString, EnumIter, FromRepr,
)]
#[repr(i64)]
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
    #[sqlx(skip)]
    pub loaded: bool,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            id: -1,
            csid: -1,
            date: Date::MIN,
            kind: NoteType::Other,
            summary: Default::default(),
            details: None,
            loaded: false,
        }
    }
}

#[async_trait]
impl Loadable for Note {
    type Id = i64;

    fn new_loadable(id: Self::Id) -> Self {
        let mut note = Self::default();
        note.id = id;
        note
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn is_loadable(&self) -> bool {
        self.id > 0
    }

    async fn do_load(&mut self, pool: &Pool<Sqlite>) -> anyhow::Result<Self> {
        Note::fetch(self.id, pool).await.map_err(|e| e.into())
    }
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
    pub fn new(
        csid: i64,
        date: Date,
        kind: NoteType,
        summary: String,
        details: Option<String>,
    ) -> Self {
        Self {
            id: -1,
            csid,
            date,
            kind,
            summary,
            details,
            loaded: false,
        }
    }
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
            .map(|mut n: Note| {
                n.loaded = true;
                n
            })
    }

    pub async fn fetch_all(
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Note>, sqlx::Error> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_all(pool)
            .await
            .map(|mut v| {
                let _ = v.iter_mut().map(|n: &mut Note| {
                    n.loaded = true;
                    n
                });
                v
            })
    }

    pub async fn insert(&self, pool: &Pool<Sqlite>) -> anyhow::Result<Note> {
        if self.summary.is_empty() {
            return Err(anyhow!("No summary specified"));
        }
        sqlx::query_as(
            r#"INSERT INTO sc_collection_sample_notes
            (csid, date, kind, summary, details)
            VALUES (?, ?, ?, ?, ?) RETURNING *"#,
        )
        .bind(self.csid)
        .bind(self.date)
        .bind(self.kind as i64)
        .bind(&self.summary)
        .bind(&self.details)
        .fetch_one(pool)
        .await
        .map(|mut n: Note| {
            n.loaded = true;
            n
        })
        .map_err(|e| e.into())
    }

    pub async fn update(&self, pool: &Pool<Sqlite>) -> Result<Note, sqlx::Error> {
        sqlx::query_as(
            r#"UPDATE sc_collection_sample_notes
            SET csid=?, date=?, kind=?, summary=?, details=? WHERE id=?
            RETURNING *"#,
        )
        .bind(self.csid)
        .bind(self.date)
        .bind(self.kind as i64)
        .bind(&self.summary)
        .bind(&self.details)
        .bind(self.id)
        .fetch_one(pool)
        .await
    }
}

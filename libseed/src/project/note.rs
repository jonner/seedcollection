use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Pool, QueryBuilder, Sqlite};
use std::sync::Arc;
use strum_macros::{EnumIter, EnumString, FromRepr};
use time::Date;

use crate::{
    error::{Error, Result},
    filter::{DynFilterPart, FilterPart},
    loadable::Loadable,
};

#[derive(
    sqlx::Type,
    Debug,
    Copy,
    Clone,
    Serialize,
    Deserialize,
    EnumString,
    EnumIter,
    FromRepr,
    PartialEq,
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
pub enum NoteFilter {
    Id(i64),
    AllocationId(i64),
}

#[derive(sqlx::FromRow, Deserialize, Serialize, Debug, PartialEq)]
pub struct Note {
    #[sqlx(rename = "pnoteid")]
    pub id: i64,
    pub psid: i64,
    #[sqlx(rename = "notedate")]
    pub date: Date,
    #[sqlx(rename = "notetype")]
    pub kind: NoteType,
    #[sqlx(rename = "notesummary")]
    pub summary: String,
    #[sqlx(rename = "notedetails")]
    pub details: Option<String>,
}

impl Default for Note {
    fn default() -> Self {
        Self {
            id: -1,
            psid: -1,
            date: Date::MIN,
            kind: NoteType::Other,
            summary: Default::default(),
            details: None,
        }
    }
}

#[async_trait]
impl Loadable for Note {
    type Id = i64;

    fn invalid_id() -> Self::Id {
        -1
    }

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id
    }

    async fn load(id: Self::Id, pool: &Pool<Sqlite>) -> Result<Self> {
        Note::fetch(id, pool).await.map_err(|e| e.into())
    }

    async fn delete_id(id: &Self::Id, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        sqlx::query!("DELETE FROM sc_project_notes WHERE pnoteid=?", id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
    }
}

impl FilterPart for NoteFilter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(i) => _ = builder.push(" pnoteid=").push_bind(*i),
            Self::AllocationId(i) => _ = builder.push(" psid=").push_bind(*i),
        }
    }
}

impl Note {
    pub fn new(
        psid: i64,
        date: Date,
        kind: NoteType,
        summary: String,
        details: Option<String>,
    ) -> Self {
        Self {
            id: -1,
            psid,
            date,
            kind,
            summary,
            details,
        }
    }
    fn build_query(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder = QueryBuilder::new(
            r#"SELECT pnoteid, psid, notedate, notetype, notesummary, notedetails FROM sc_project_notes"#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" ORDER BY psid, notedate");
        tracing::debug!("GENERATED SQL: {}", builder.sql());
        builder
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> Result<Note, sqlx::Error> {
        Self::build_query(Some(Arc::new(NoteFilter::Id(id))))
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

    pub async fn insert(&self, pool: &Pool<Sqlite>) -> Result<Note> {
        if self.summary.is_empty() {
            return Err(Error::InvalidData("No summary specified".to_string()));
        }
        sqlx::query_as(
            r#"INSERT INTO sc_project_notes
            (psid, notedate, notetype, notesummary, notedetails)
            VALUES (?, ?, ?, ?, ?) RETURNING *"#,
        )
        .bind(self.psid)
        .bind(self.date)
        .bind(self.kind as i64)
        .bind(&self.summary)
        .bind(&self.details)
        .fetch_one(pool)
        .await
        .map_err(|e| e.into())
    }

    pub async fn update(&self, pool: &Pool<Sqlite>) -> Result<Note, sqlx::Error> {
        sqlx::query_as(
            r#"UPDATE sc_project_notes
            SET psid=?, notedate=?, notetype=?, notesummary=?, notedetails=? WHERE pnoteid=?
            RETURNING *"#,
        )
        .bind(self.psid)
        .bind(self.date)
        .bind(self.kind as i64)
        .bind(&self.summary)
        .bind(&self.details)
        .bind(self.id)
        .fetch_one(pool)
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;
    use time::Month;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../../db/fixtures",
            scripts("users", "sources", "taxa", "csnotes")
        )
    ))]
    async fn test_query_notes(pool: Pool<Sqlite>) {
        let mut note = Note::fetch(3, &pool).await.expect("Failed to load notes");
        tracing::debug!("{note:?}");
        assert_eq!(note.id, 3);
        assert_eq!(note.psid, 2);
        assert_eq!(note.date.year(), 2024);
        assert_eq!(note.date.month(), Month::January);
        assert_eq!(note.date.day(), 16);
        assert_eq!(note.kind, NoteType::Preparation);
        assert_eq!(note.summary, "summary 3");
        assert_eq!(note.details, Some("details 3".to_string()));

        note.summary = "I changed the summary".to_string();
        note.details = None;
        note.date = note.date.replace_year(2019).expect("Unable to update date");
        note.update(&pool).await.expect("Couldn't update the note");
        let loaded = Note::load(note.id, &pool)
            .await
            .expect("Failed to load new note");
        assert_eq!(note, loaded);

        // fetch all notes for a sample
        let notes = Note::fetch_all(Some(Arc::new(NoteFilter::AllocationId(1))), &pool)
            .await
            .expect("Unable to load notes for sample");
        assert_eq!(notes.len(), 2);
        // they should be sorted by date
        assert_eq!(notes[0].id, 2);
        assert_eq!(notes[1].id, 1);
        assert!(notes[0].date < notes[1].date);
    }
}

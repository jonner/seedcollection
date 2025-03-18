//! Notes associated with project allocations
use crate::core::{
    database::Database,
    error::{Error, Result},
    loadable::Loadable,
    query::{DynFilterPart, FilterPart},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, sqlite::SqliteQueryResult};
use std::sync::Arc;
use strum_macros::EnumIter;
use time::Date;
use tracing::debug;

/// A category for a note
#[derive(sqlx::Type, Debug, Copy, Clone, Serialize, Deserialize, EnumIter, PartialEq)]
#[repr(i64)]
pub enum NoteType {
    /// This note is related to preparation of the seed sample
    Preparation = 1,

    /// This note is related to the germination of the seed sample
    Germination = 2,

    /// This note is related to the planting of the seed sample
    Planting = 3,

    /// This note is related to the growth of the seed sample
    Growing = 4,

    /// This note is related to something else
    Other = 5,
}

/// A type for specifying fields to filter when querying notes from the database
#[derive(Clone)]
pub enum NoteFilter {
    /// Filter against the ID of the note
    Id(i64),

    /// Filter against the ID of the associated [Allocation](super::Allocation) object
    AllocationId(i64),
}

/// An object that represents a project-specific note tied to a particular [Allocation](super::Allocation) object
#[derive(sqlx::FromRow, Deserialize, Serialize, Debug, PartialEq)]
pub struct Note {
    /// A unique ID that identifies this note in the database
    #[sqlx(rename = "pnoteid")]
    pub id: i64,

    /// the ID of the allocation that is associated with this note
    pub psid: i64,

    /// The date that this note was added to the database
    #[sqlx(rename = "notedate")]
    pub date: Date,

    /// The category of the note
    #[sqlx(rename = "notetype")]
    pub kind: NoteType,

    /// A short summary (headline) of the note
    #[sqlx(rename = "notesummary")]
    pub summary: String,

    /// The body of the note
    #[sqlx(rename = "notedetails")]
    pub details: Option<String>,
}

#[async_trait]
impl Loadable for Note {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id
    }

    async fn load(id: Self::Id, db: &Database) -> Result<Self> {
        Self::query_builder(Some(Arc::new(NoteFilter::Id(id))))
            .build_query_as()
            .fetch_one(db.pool())
            .await
            .map_err(|e| e.into())
    }

    async fn delete_id(id: &Self::Id, db: &Database) -> Result<SqliteQueryResult> {
        sqlx::query!("DELETE FROM sc_project_notes WHERE pnoteid=?", id)
            .execute(db.pool())
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
    /// Create a new Note. It will initially have an invalid ID until it is inserted into the database.
    pub fn new(
        psid: i64,
        date: Date,
        kind: NoteType,
        summary: String,
        details: Option<String>,
    ) -> Self {
        Self {
            id: Self::invalid_id(),
            psid,
            date,
            kind,
            summary,
            details,
        }
    }

    fn query_builder(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
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

    /// Load all matching notes from the database
    pub async fn load_all(
        filter: Option<DynFilterPart>,
        db: &Database,
    ) -> Result<Vec<Note>, sqlx::Error> {
        Self::query_builder(filter)
            .build_query_as()
            .fetch_all(db.pool())
            .await
    }

    /// Insert this note into the database. If successful, the ID of the note
    /// object will be updated to match the ID of the newly-inserted row.
    pub async fn insert(&self, db: &Database) -> Result<Note> {
        if self.summary.is_empty() {
            return Err(Error::InvalidStateMissingAttribute("summary".to_string()));
        }
        debug!(?self, "Inserting note into database");
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
        .fetch_one(db.pool())
        .await
        .map_err(|e| e.into())
    }

    /// Update the note in the database such that it matches this object
    pub async fn update(&self, db: &Database) -> Result<Note, sqlx::Error> {
        debug!(?self, "Updating note in database");
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
        .fetch_one(db.pool())
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Pool;
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
        let db = Database::from(pool);
        let mut note = Note::load(3, &db).await.expect("Failed to load notes");
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
        note.update(&db).await.expect("Couldn't update the note");
        let loaded = Note::load(note.id, &db)
            .await
            .expect("Failed to load new note");
        assert_eq!(note, loaded);

        // fetch all notes for a sample
        let notes = Note::load_all(Some(Arc::new(NoteFilter::AllocationId(1))), &db)
            .await
            .expect("Unable to load notes for sample");
        assert_eq!(notes.len(), 2);
        // they should be sorted by date
        assert_eq!(notes[0].id, 2);
        assert_eq!(notes[1].id, 1);
        assert!(notes[0].date < notes[1].date);
    }
}

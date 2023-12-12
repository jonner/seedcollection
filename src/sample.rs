use crate::location;
use crate::taxonomy;
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, Row};

pub struct Sample {
    pub id: i64,
    pub taxon: taxonomy::Taxon,
    pub location: location::Location,
    pub quantity: Option<i64>,
    pub month: Option<u32>,
    pub year: Option<u32>,
    pub notes: Option<String>,
}

impl FromRow<'_, SqliteRow> for Sample {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            taxon: taxonomy::Taxon::from_row(row)?,
            location: location::Location::from_row(row)?,
            quantity: row.try_get("quantity").unwrap_or(None),
            month: row.try_get("month").unwrap_or(None),
            year: row.try_get("year").unwrap_or(None),
            notes: row.try_get("notes").unwrap_or(None),
        })
    }
}

use crate::taxonomy;
use crate::location;
use sqlx::{FromRow, Row};
use sqlx::sqlite::SqliteRow;

pub struct Sample {
    pub id: i64,
    pub taxon: Option<taxonomy::Taxon>,
    pub location: Option<location::Location>,
}

impl FromRow<'_, SqliteRow> for Sample {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            taxon: Some(taxonomy::Taxon::from_row(row)?),
            location: Some(location::Location::from_row(row)?),
        })
    }
}

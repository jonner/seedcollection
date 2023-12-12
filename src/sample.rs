use crate::location;
use crate::taxonomy;
use sqlx::sqlite::SqliteRow;
use sqlx::{FromRow, Row};

pub struct Sample {
    pub id: i64,
    pub taxon: Option<taxonomy::Taxon>,
    pub location: Option<location::Location>,
    pub quantity: Option<i64>,
}

impl FromRow<'_, SqliteRow> for Sample {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            taxon: Some(taxonomy::Taxon::from_row(row)?),
            location: Some(location::Location::from_row(row)?),
            quantity: Some(row.try_get("quantity")?),
        })
    }
}

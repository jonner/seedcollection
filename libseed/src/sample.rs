use crate::{location::Location, taxonomy::Taxon};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, FromRow, QueryBuilder, Row, Sqlite};

#[derive(Deserialize, Serialize)]
pub struct Sample {
    pub id: i64,
    pub taxon: Taxon,
    pub location: Location,
    pub quantity: Option<i64>,
    pub month: Option<u32>,
    pub year: Option<u32>,
    pub notes: Option<String>,
}

pub fn build_query(collectionid: Option<i64>) -> QueryBuilder<'static, Sqlite> {
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
        r#"SELECT S.id, T.tsn, L.locid, L.name as locname, T.complete_name,
        T.unit_name1, T.unit_name2, T.unit_name3,
                    quantity, month, year, notes
                    FROM seedsamples S
                    INNER JOIN taxonomic_units T ON T.tsn=S.tsn
                    INNER JOIN seedlocations L on L.locid=S.collectedlocation"#,
    );
    if let Some(id) = collectionid {
        builder.push(
            " INNER JOIN seedcollectionsamples CS ON CS.sampleid=S.id WHERE cs.collectionid=",
        );
        builder.push_bind(id);
    }
    builder
}

impl FromRow<'_, SqliteRow> for Sample {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            taxon: Taxon::from_row(row)?,
            location: Location::from_row(row)?,
            quantity: row.try_get("quantity").unwrap_or(None),
            month: row.try_get("month").unwrap_or(None),
            year: row.try_get("year").unwrap_or(None),
            notes: row.try_get("notes").unwrap_or(None),
        })
    }
}

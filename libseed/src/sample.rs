use crate::{location::Location, taxonomy::Taxon};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, FromRow, QueryBuilder, Row, Sqlite};

#[derive(Deserialize, Serialize, Debug, sqlx::Type)]
#[repr(i32)]
pub enum Certainty {
    Certain = 1,
    Uncertain = 2,
}

#[derive(Deserialize, Serialize)]
pub struct Sample {
    pub id: i64,
    pub taxon: Taxon,
    pub location: Location,
    pub quantity: Option<i64>,
    pub month: Option<u32>,
    pub year: Option<u32>,
    pub notes: Option<String>,
    pub collection: Option<i64>,
    pub certainty: Certainty,
}

pub enum Filter {
    Collection(i64),
    NoCollection,
    Sample(i64),
    Location(i64),
    Taxon(i64),
    TaxonNameLike(String),
}

pub fn build_query(filter: Option<Filter>) -> QueryBuilder<'static, Sqlite> {
    let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
        r#"SELECT S.id, T.tsn, T.parent_tsn as parentid, L.locid, L.name as locname, T.complete_name,
        T.unit_name1, T.unit_name2, T.unit_name3, T.phylo_sort_seq as seq,
                    quantity, month, year, notes, certainty, CS.collectionid,
                    GROUP_CONCAT(V.vernacular_name, "@") as cnames
                    FROM seedsamples S
                    INNER JOIN taxonomic_units T ON T.tsn=S.tsn
                    INNER JOIN seedlocations L on L.locid=S.collectedlocation
                    LEFT JOIN seedcollectionsamples CS ON CS.sampleid=S.id
                    LEFT JOIN (SELECT * FROM vernaculars WHERE
                    (language="English" or language="unspecified")) V on V.tsn=T.tsn
                    "#,
    );
    match &filter {
        Some(f) => match f {
            Filter::Collection(id) => {
                builder.push(" WHERE cs.collectionid=");
                builder.push_bind(id.clone());
            }
            Filter::NoCollection => {
                builder.push(" WHERE cs.collectionid IS NULL");
            }
            Filter::Sample(id) => {
                builder.push(" WHERE S.id=");
                builder.push_bind(id.clone());
            }
            Filter::Location(id) => {
                builder.push(" WHERE L.locid=");
                builder.push_bind(id.clone());
            }
            Filter::Taxon(id) => {
                builder.push(" WHERE S.tsn=");
                builder.push_bind(id.clone());
            }
            Filter::TaxonNameLike(substr) => {
                if !substr.is_empty() {
                    let wildcard = format!("%{substr}%");
                    builder.push(" WHERE T.unit_name1 LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(" OR T.unit_name2 LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(" OR T.unit_name3 LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(" OR V.vernacular_name LIKE ");
                    builder.push_bind(wildcard.clone());
                }
            }
        },
        None => (),
    }
    builder.push(" GROUP BY S.id, T.tsn ORDER BY phylo_sort_seq");
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
            collection: row.try_get("collectionid").unwrap_or(None),
            certainty: row.try_get("certainty").unwrap_or(Certainty::Uncertain),
        })
    }
}

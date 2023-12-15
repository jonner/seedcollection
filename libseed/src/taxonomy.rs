use log::debug;
use serde::{Deserialize, Serialize};
use sqlx::{error::Error::ColumnDecode, sqlite::SqliteRow, FromRow, Row};
use std::str::FromStr;
use strum_macros::{Display, EnumIter, EnumString, FromRepr};

pub const KINGDOM_PLANTAE: i64 = 3;

#[derive(Debug, Clone, Display, EnumString, EnumIter, FromRepr, Deserialize, Serialize)]
#[strum(ascii_case_insensitive)]
pub enum Rank {
    Unknown = 0,
    Kingdom = 10,
    Subkingdom = 20,
    Infrakingdom = 25,
    Superdivision = 27,
    Division = 30,
    Subdivision = 40,
    Infradivision = 45,
    Superclass = 50,
    Class = 60,
    Subclass = 70,
    Infraclass = 80,
    Superorder = 90,
    Order = 100,
    Suborder = 110,
    Family = 140,
    Subfamily = 150,
    Tribe = 160,
    Subtribe = 170,
    Genus = 180,
    Subgenus = 190,
    Section = 200,
    Subsection = 210,
    Species = 220,
    Subspecies = 230,
    Variety = 240,
    Subvariety = 250,
    Form = 260,
    Subform = 270,
}

#[derive(Debug, Display, EnumString, FromRepr, Serialize, Deserialize)]
pub enum NativeStatus {
    #[strum(serialize = "Native", serialize = "N")]
    Native,
    #[strum(serialize = "Introduced", serialize = "I")]
    Introduced,
    #[strum(serialize = "Unknown", serialize = "U")]
    Unknown,
}

#[derive(Deserialize, Serialize)]
pub struct Taxon {
    pub id: i64,
    pub rank: Rank,
    pub name1: Option<String>,
    pub name2: Option<String>,
    pub name3: Option<String>,
    pub complete_name: String,
    pub vernaculars: Vec<String>,
    pub native_status: Option<NativeStatus>,
}

impl FromRow<'_, SqliteRow> for Taxon {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        let rank = match row.try_get::<i64, _>("rank_id") {
            Err(_) => Rank::Unknown,
            Ok(r) => {
                let rankid: usize =
                    r.try_into()
                        .map_err(|e: std::num::TryFromIntError| ColumnDecode {
                            index: "rank".to_string(),
                            source: e.into(),
                        })?;
                Rank::from_repr(rankid).ok_or_else(|| ColumnDecode {
                    index: "rank".to_string(),
                    source: Box::new(strum::ParseError::VariantNotFound),
                })?
            }
        };
        let status = match row.try_get("native_status") {
            Err(_) => None,
            Ok("") => None,
            Ok(val) => Some(NativeStatus::from_str(val).map_err(|e| ColumnDecode {
                index: "native_status".to_string(),
                source: e.into(),
            })?),
        };
        let vernaculars = match row.try_get::<&str, _>("cnames") {
            Err(_) => Vec::new(),
            Ok(s) => {
                let splits = s.split('@').map(|x| x.to_string());
                splits.collect::<Vec<String>>()
            }
        };
        Ok(Self {
            id: row.try_get("tsn")?,
            rank,
            complete_name: row.try_get("complete_name")?,
            vernaculars,
            name1: row.try_get("unit_name1")?,
            name2: row.try_get("unit_name2")?,
            name3: row.try_get("unit_name3")?,
            native_status: status,
        })
    }
}

pub fn build_query(
    tsn: Option<i64>,
    rank: Option<Rank>,
    genus: Option<String>,
    species: Option<String>,
    any: Option<String>,
    minnesota: bool,
) -> sqlx::QueryBuilder<'static, sqlx::Sqlite> {
    let mut builder: sqlx::QueryBuilder<sqlx::Sqlite> = sqlx::QueryBuilder::new(
        r#"SELECT T.tsn, T.unit_name1, T.unit_name2, T.unit_name3, T.complete_name, T.rank_id, M.native_status,
            GROUP_CONCAT(V.vernacular_name, "@") as cnames
            FROM taxonomic_units T
            LEFT JOIN (SELECT * FROM vernaculars WHERE
                       (language="English" or language="unspecified")) V on V.tsn=T.tsn"#,
    );

    if minnesota {
        builder.push(" INNER JOIN mntaxa M on T.tsn=M.tsn ");
    } else {
        builder.push(" LEFT JOIN mntaxa M on T.tsn=M.tsn ");
    }

    builder.push(r#" WHERE name_usage="accepted" AND kingdom_id="#);
    builder.push_bind(KINGDOM_PLANTAE);
    if let Some(id) = tsn {
        builder.push(" AND T.tsn=");
        builder.push_bind(id);
    }
    if let Some(rank) = rank {
        builder.push(" AND rank_id=");
        builder.push_bind(rank as i64);
    }
    if let Some(genus) = genus {
        builder.push(" AND unit_name1 LIKE ");
        builder.push_bind(genus);
    }
    if let Some(species) = species {
        builder.push(" AND unit_name2 LIKE ");
        builder.push_bind(species);
    }

    if let Some(any) = any {
        builder.push(" AND (");
        let any = format!("%{any}%");
        let fields = [
            "unit_name1",
            "unit_name2",
            "unit_name3",
            "V.vernacular_name",
        ];
        let mut first = true;
        for field in fields {
            if !first {
                builder.push(" OR");
            }
            first = false;
            builder.push(format!(" {field} LIKE "));
            builder.push_bind(any.clone());
        }
        builder.push(" )");
    }

    builder.push(" GROUP BY T.tsn");
    debug!("generated sql: <<{}>>", builder.sql());
    builder
}

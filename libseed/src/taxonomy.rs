use anyhow::Result;
use log::debug;
use serde::{Deserialize, Serialize};
use sqlx::{error::Error::ColumnDecode, sqlite::SqliteRow, FromRow, Pool, Row, Sqlite};
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
    pub parentid: Option<i64>,
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
            Ok(s) if !s.is_empty() => {
                let splits = s.split('@').map(|x| x.to_string());
                splits.collect::<Vec<String>>()
            }
            _ => Vec::new(),
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
            parentid: row.try_get("parentid")?,
        })
    }
}

#[derive(Deserialize, Clone)]
pub enum FilterField {
    Id(i64),
    Rank(Rank),
    Genus(String),
    Species(String),
    Name1(String),
    Name2(String),
    Name3(String),
    Vernacular(String),
    Minnesota(bool),
    ParentId(i64),
}

pub enum FilterOperation {
    Or,
    And,
}

pub struct CompoundFilterCondition {
    conditions: Vec<Box<dyn FilterQueryBuilder>>,
    op: FilterOperation,
}

impl CompoundFilterCondition {
    pub fn new(op: FilterOperation, fields: Vec<Box<dyn FilterQueryBuilder>>) -> Self {
        Self {
            conditions: fields,
            op,
        }
    }

    pub fn add_field(&mut self, field: FilterField) {
        self.conditions.push(Box::new(field))
    }
}

pub trait FilterQueryBuilder: Send {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<'static, sqlx::Sqlite>);
}

impl FilterQueryBuilder for FilterField {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<'static, sqlx::Sqlite>) {
        match self {
            Self::Id(n) => builder.push("T.tsn=").push_bind(n.clone()),
            Self::ParentId(n) => builder.push("T.parent_tsn=").push_bind(n.clone()),
            Self::Genus(s) => builder.push("T.unit_name1=").push_bind(s.clone()),
            Self::Species(s) => builder.push("T.unit_name2=").push_bind(s.clone()),
            Self::Rank(rank) => builder.push("T.rank_id=").push_bind(rank.clone() as i64),
            Self::Name1(s) => builder
                .push("T.unit_name1 LIKE ")
                .push_bind(format!("%{s}%")),
            Self::Name2(s) => builder
                .push("T.unit_name2 LIKE ")
                .push_bind(format!("%{s}%")),
            Self::Name3(s) => builder
                .push("T.unit_name3 LIKE ")
                .push_bind(format!("%{s}%")),
            Self::Vernacular(s) => builder.push("V.vernacular_name LIKE ").push_bind(s.clone()),
            Self::Minnesota(val) => match val {
                true => builder.push("M.tsn IS NOT NULL"),
                false => builder.push("M.tsn IS NULL"),
            },
        };
    }
}

pub fn filter_by(
    id: Option<i64>,
    rank: Option<Rank>,
    genus: Option<String>,
    species: Option<String>,
    any: Option<String>,
    minnesota: Option<bool>,
) -> Option<Box<dyn FilterQueryBuilder>> {
    let mut fields: Vec<Box<dyn FilterQueryBuilder>> = Vec::new();
    if let Some(id) = id {
        fields.push(Box::new(FilterField::Id(id)));
    }
    if let Some(rank) = rank {
        fields.push(Box::new(FilterField::Rank(rank)));
    }
    if let Some(genus) = genus {
        fields.push(Box::new(FilterField::Genus(genus)));
    }
    if let Some(species) = species {
        fields.push(Box::new(FilterField::Species(species)));
    }
    if let Some(s) = any {
        fields.push(Box::new(any_filter(&s)));
    }
    if let Some(val) = minnesota {
        fields.push(Box::new(FilterField::Minnesota(val)));
    }

    if fields.is_empty() {
        None
    } else {
        Some(Box::new(CompoundFilterCondition::new(
            FilterOperation::And,
            fields,
        )))
    }
}

pub fn any_filter(s: &str) -> CompoundFilterCondition {
    let mut fields: Vec<Box<dyn FilterQueryBuilder>> = Vec::new();
    fields.push(Box::new(FilterField::Name1(s.to_string())));
    fields.push(Box::new(FilterField::Name2(s.to_string())));
    fields.push(Box::new(FilterField::Name3(s.to_string())));
    CompoundFilterCondition::new(FilterOperation::Or, fields)
}

impl FilterQueryBuilder for CompoundFilterCondition {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<'static, sqlx::Sqlite>) {
        let mut first = true;
        builder.push(" (");
        let separator = match self.op {
            FilterOperation::And => " AND ",
            FilterOperation::Or => " OR ",
        };

        for cond in &self.conditions {
            if first {
                first = false;
            } else {
                builder.push(separator);
            }
            cond.add_to_query(builder);
        }
        builder.push(")");
    }
}

pub fn build_query(
    filter: Option<Box<dyn FilterQueryBuilder>>,
    limit: Option<i64>,
) -> sqlx::QueryBuilder<'static, sqlx::Sqlite> {
    let mut builder: sqlx::QueryBuilder<sqlx::Sqlite> = sqlx::QueryBuilder::new(
        r#"SELECT T.tsn, T.parent_tsn as parentid, T.unit_name1, T.unit_name2, T.unit_name3, T.complete_name, T.rank_id, M.native_status,
            GROUP_CONCAT(V.vernacular_name, "@") as cnames
            FROM taxonomic_units T
            LEFT JOIN (SELECT * FROM vernaculars WHERE
                       (language="English" or language="unspecified")) V on V.tsn=T.tsn
     LEFT JOIN mntaxa M on T.tsn=M.tsn 
      WHERE name_usage="accepted" AND kingdom_id="#,
    );
    builder.push_bind(KINGDOM_PLANTAE);

    if let Some(filter) = filter {
        builder.push(" AND ");
        filter.add_to_query(&mut builder);
    }

    builder.push(" GROUP BY T.tsn");
    if let Some(n) = limit {
        builder.push(" LIMIT ");
        builder.push_bind(n);
    }
    debug!("generated sql: <<{}>>", builder.sql());
    builder
}

pub async fn fetch_taxon(id: i64, pool: &Pool<Sqlite>) -> Result<Taxon> {
    let mut query = build_query(Some(Box::new(FilterField::Id(id.clone()))), None);
    Ok(query.build_query_as().fetch_one(pool).await?)
}

pub async fn fetch_taxon_hierarchy(id: i64, pool: &Pool<Sqlite>) -> Result<Vec<Taxon>> {
    let mut hierarchy = Vec::new();
    let mut taxon = fetch_taxon(id, pool).await?;
    loop {
        let parentid = taxon.parentid;
        hierarchy.push(taxon);
        match parentid {
            Some(id) if id > 0 => {
                taxon = fetch_taxon(id, pool).await?;
            }
            _ => break,
        }
    }

    Ok(hierarchy)
}

pub async fn fetch_children(id: i64, pool: &Pool<Sqlite>) -> Result<Vec<Taxon>> {
    let filter: Option<Box<dyn FilterQueryBuilder>> = Some(Box::new(FilterField::ParentId(id)));
    let mut query = build_query(filter, None);
    Ok(query.build_query_as().fetch_all(pool).await?)
}

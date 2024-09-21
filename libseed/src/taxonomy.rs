//! Objects related to querying the taxonomic database

use async_trait::async_trait;
use serde::{de::IntoDeserializer, Deserialize, Serialize};
use sqlx::{
    error::Error::ColumnDecode,
    sqlite::{SqliteQueryResult, SqliteRow},
    FromRow, Pool, Row, Sqlite,
};
use std::{str::FromStr, sync::Arc};
use strum_macros::{Display, EnumIter, FromRepr};
use tracing::debug;

use crate::{
    error::Result,
    filter::{CompoundFilter, DynFilterPart, FilterPart, LimitSpec, Op},
    loadable::{ExternalRef, Loadable},
    Error,
};

pub const KINGDOM_PLANTAE: i64 = 3;

#[derive(Debug, Clone, Display, EnumIter, FromRepr, Deserialize, Serialize, PartialEq)]
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

impl FromStr for Rank {
    type Err = serde::de::value::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let deserializer = s.into_deserializer();
        Deserialize::deserialize(deserializer)
    }
}

#[derive(Debug, Display, FromRepr, Serialize, Deserialize, PartialEq, Clone)]
pub enum NativeStatus {
    #[serde(alias = "N")]
    Native,
    #[serde(alias = "I")]
    Introduced,
    #[serde(alias = "U")]
    Unknown,
}

impl FromStr for NativeStatus {
    type Err = serde::de::value::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let deserializer = s.into_deserializer();
        Deserialize::deserialize(deserializer)
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
/// An object representing a particular taxon from the database
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
    pub seq: Option<i64>,
    pub germination: Option<Vec<Germination>>,
}

#[async_trait]
impl Loadable for Taxon {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id
    }

    async fn load(id: Self::Id, pool: &Pool<Sqlite>) -> Result<Self> {
        let mut query = Taxon::build_query(Some(Filter::Id(id).into()), None);
        Ok(query.build_query_as().fetch_one(pool).await?)
    }

    async fn delete_id(_id: &Self::Id, _pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        Err(Error::InvalidOperation("Cannot delete taxon".to_string()))
    }
}

#[derive(FromRow, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Germination {
    #[sqlx(rename = "germid")]
    pub id: i64,
    pub code: String,
    pub summary: Option<String>,
    pub description: Option<String>,
}

impl Germination {
    pub async fn load_all(pool: &Pool<Sqlite>) -> Result<Vec<Germination>, sqlx::Error> {
        sqlx::query_as("SELECT * FROM sc_germination_codes")
            .fetch_all(pool)
            .await
    }

    pub async fn load(id: i64, pool: &Pool<Sqlite>) -> Result<Germination, sqlx::Error> {
        sqlx::query_as("SELECT * FROM sc_germination_codes WHERE germid=?")
            .bind(id)
            .fetch_one(pool)
            .await
    }

    pub async fn update(&self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(Error::InvalidUpdateObjectNotFound);
        }
        sqlx::query(
            "UPDATE sc_germination_codes SET code=?, summary=?, description=? WHERE germid=?",
        )
        .bind(&self.code)
        .bind(&self.summary)
        .bind(&self.description)
        .bind(self.id)
        .execute(pool)
        .await
        .map_err(Into::into)
    }
}

impl FromRow<'_, SqliteRow> for ExternalRef<Taxon> {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Taxon::from_row(row)
            .map(ExternalRef::Object)
            .or_else(|_| row.try_get("tsn").map(ExternalRef::Stub))
    }
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
            seq: row.try_get("seq").unwrap_or(None),
            germination: None,
        })
    }
}

impl From<Filter> for DynFilterPart {
    fn from(value: Filter) -> Self {
        Arc::new(value)
    }
}

#[derive(Deserialize, Clone)]
pub enum Filter {
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

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(n) => builder.push("T.tsn=").push_bind(*n),
            Self::ParentId(n) => builder.push("T.parent_tsn=").push_bind(*n),
            Self::Genus(s) => builder.push("T.unit_name1 LIKE ").push_bind(s.clone()),
            Self::Species(s) => builder.push("T.unit_name2 LIKE ").push_bind(s.clone()),
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
            Self::Vernacular(s) => builder
                .push("V.vernacular_name LIKE ")
                .push_bind(format!("%{s}%")),
            Self::Minnesota(val) => match val {
                true => builder.push("M.tsn IS NOT NULL"),
                false => builder.push("M.tsn IS NULL"),
            },
        };
    }
}

pub fn match_any_name(s: &str) -> DynFilterPart {
    CompoundFilter::builder(Op::Or)
        .push(Filter::Name1(s.to_string()))
        .push(Filter::Name2(s.to_string()))
        .push(Filter::Name3(s.to_string()))
        .push(Filter::Vernacular(s.to_string()))
        .build()
}

impl Taxon {
    pub async fn fetch_hierarchy(&self, pool: &Pool<Sqlite>) -> Result<Vec<Self>> {
        let mut hierarchy = Vec::new();
        let mut taxon = Taxon::load(self.id, pool).await?;
        loop {
            let parentid = taxon.parentid;
            hierarchy.push(taxon);
            match parentid {
                Some(id) if id > 0 => {
                    taxon = Taxon::load(id, pool).await?;
                }
                _ => break,
            }
        }

        Ok(hierarchy)
    }

    pub async fn fetch_children(&self, pool: &Pool<Sqlite>) -> Result<Vec<Self>> {
        let mut query = Taxon::build_query(Some(Filter::ParentId(self.id).into()), None);
        Ok(query.build_query_as().fetch_all(pool).await?)
    }

    fn build_query(
        filter: Option<DynFilterPart>,
        limit: Option<LimitSpec>,
    ) -> sqlx::QueryBuilder<'static, sqlx::Sqlite> {
        let mut builder: sqlx::QueryBuilder<sqlx::Sqlite> = sqlx::QueryBuilder::new(
            r#"SELECT
                T.tsn,
                T.parent_tsn as parentid,
                T.unit_name1,
                T.unit_name2,
                T.unit_name3,
                T.complete_name,
                T.rank_id,
                T.phylo_sort_seq as seq,
                M.native_status,
                GROUP_CONCAT(V.vernacular_name, "@") as cnames
            FROM taxonomic_units T
            LEFT JOIN (
                SELECT *
                FROM vernaculars
                WHERE ( language="English" OR language="unspecified" )
            ) V on V.tsn=T.tsn
            LEFT JOIN mntaxa M on T.tsn=M.tsn 
            WHERE name_usage="accepted" AND kingdom_id="#,
        );
        builder.push_bind(KINGDOM_PLANTAE);

        if let Some(filter) = filter {
            builder.push(" AND ");
            filter.add_to_query(&mut builder);
        }

        builder.push(" GROUP BY T.tsn ORDER BY phylo_sort_seq");
        if let Some(LimitSpec(count, offset)) = limit {
            builder.push(" LIMIT ");
            builder.push_bind(count);
            if let Some(offset) = offset {
                builder.push(" OFFSET ");
                builder.push_bind(offset);
            }
        }
        debug!("generated sql: <<{}>>", builder.sql());
        builder
    }

    pub async fn load_all(
        filter: Option<DynFilterPart>,
        limit: Option<LimitSpec>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Taxon>, sqlx::Error> {
        Taxon::build_query(filter, limit)
            .build_query_as()
            .fetch_all(pool)
            .await
    }

    pub async fn load_germination_info(&mut self, pool: &Pool<Sqlite>) -> Result<()> {
        self.germination = Some(
            sqlx::query_as(
                r#"SELECT G.* from sc_germination_codes G
            INNER JOIN sc_taxon_germination TG ON TG.germid=G.germid
            WHERE TG.tsn=?"#,
            )
            .bind(self.id)
            .fetch_all(pool)
            .await?,
        );
        Ok(())
    }

    pub fn build_count(filter: Option<DynFilterPart>) -> sqlx::QueryBuilder<'static, sqlx::Sqlite> {
        let mut builder: sqlx::QueryBuilder<sqlx::Sqlite> = sqlx::QueryBuilder::new(
            r#"SELECT COUNT(tsn) as count
            FROM taxonomic_units T
      WHERE name_usage="accepted" AND kingdom_id="#,
        );
        builder.push_bind(KINGDOM_PLANTAE);

        if let Some(filter) = filter {
            builder.push(" AND ");
            filter.add_to_query(&mut builder);
        }

        builder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

    const CANADA_WILD_RYE: i64 = 40683;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("taxa"))
    ))]
    async fn fetch_taxon(pool: Pool<Sqlite>) {
        let taxon = Taxon::load(CANADA_WILD_RYE, &pool)
            .await
            .expect("Unable to load taxon");
        assert_eq!(taxon.name1, Some("Elymus".to_string()));
        assert_eq!(taxon.name2, Some("canadensis".to_string()));
        assert_eq!(taxon.rank, Rank::Species);
        assert!(taxon
            .vernaculars
            .iter()
            .find(|v| v == &"Canada wildrye")
            .is_some());
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("taxa"))
    ))]
    async fn fetch_many(pool: Pool<Sqlite>) {
        let taxa = Taxon::load_all(
            Some(Filter::Genus("Elymus".to_string()).into()),
            None,
            &pool,
        )
        .await
        .expect("Unable to load taxon");
        assert_eq!(taxa.len(), 2);
        assert_eq!(taxa[0].name1, Some("Elymus".to_string()));
        assert_eq!(taxa[0].name2, None);
        assert_eq!(taxa[0].rank, Rank::Genus);
        assert!(taxa[0]
            .vernaculars
            .iter()
            .find(|v| v == &"wildrye")
            .is_some());
        assert_eq!(taxa[1].name1, Some("Elymus".to_string()));
        assert_eq!(taxa[1].name2, Some("canadensis".to_string()));
        assert_eq!(taxa[1].rank, Rank::Species);
        assert!(taxa[1]
            .vernaculars
            .iter()
            .find(|v| v == &"Canada wildrye")
            .is_some());
    }
}

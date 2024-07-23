//! Objects to keep track of samples of seeds that were collected or purchased
use crate::{
    error::{Error, Result},
    filter::{Cmp, CompoundFilter, DynFilterPart, FilterOp, FilterPart},
    loadable::{ExternalRef, Loadable},
    source::Source,
    taxonomy::Taxon,
    user::User,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteQueryResult, SqliteRow},
    FromRow, Pool, QueryBuilder, Row, Sqlite,
};
use std::sync::Arc;
use strum_macros::Display;

#[derive(Clone, Deserialize, Serialize, Debug, sqlx::Type, PartialEq, Display)]
#[repr(i32)]
pub enum Certainty {
    Certain = 1,
    Uncertain = 2,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Sample {
    pub id: i64,
    pub user: ExternalRef<User>,
    pub taxon: ExternalRef<Taxon>,
    pub source: ExternalRef<Source>,
    pub quantity: Option<i64>,
    pub month: Option<u32>,
    pub year: Option<u32>,
    pub notes: Option<String>,
    pub certainty: Certainty,
}

#[derive(Clone)]
pub enum Filter {
    Id(Cmp, i64),
    IdNotIn(Vec<i64>),
    SourceId(Cmp, i64),
    SourceNameLike(String),
    TaxonId(Cmp, i64),
    TaxonNameLike(String),
    UserId(i64),
    Notes(Cmp, String),
}

#[async_trait]
impl Loadable for Sample {
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
        Sample::fetch(id, pool).await
    }

    async fn delete_id(id: &Self::Id, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        sqlx::query("DELETE FROM sc_samples WHERE sampleid=?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
    }
}

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(cmp, id) => _ = builder.push("sampleid").push(cmp).push_bind(*id),
            Self::IdNotIn(list) => {
                _ = builder.push("sampleid NOT IN (");
                let mut sep = builder.separated(", ");
                for id in list {
                    sep.push_bind(*id);
                }
                builder.push(")");
            }
            Self::SourceId(cmp, id) => _ = builder.push("srcid").push(cmp).push_bind(*id),
            Self::TaxonId(cmp, id) => _ = builder.push("tsn").push(cmp).push_bind(*id),
            Self::TaxonNameLike(s) => {
                if !s.is_empty() {
                    let wildcard = format!("%{s}%");
                    builder.push(" (");
                    builder.push(" unit_name1 LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(" OR unit_name2 LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(" OR unit_name3 LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(" OR cnames LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(") ");
                }
            }
            Self::UserId(id) => _ = builder.push("userid=").push_bind(*id),
            Self::Notes(cmp, s) => _ = builder.push("notes").push(cmp).push_bind(format!("%{s}%")),
            Self::SourceNameLike(s) => {
                if !s.is_empty() {
                    let wildcard = format!("%{s}%");
                    builder.push(" srcname LIKE ");
                    builder.push_bind(wildcard);
                }
            }
        };
    }
}

pub enum Sort {
    Id,
    TaxonName,
    TaxonSequence,
    SourceId,
    SourceName,
}

impl Sample {
    fn build_query(
        filter: Option<DynFilterPart>,
        sort: Option<Sort>,
    ) -> QueryBuilder<'static, Sqlite> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT * FROM vsamples");
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" ORDER BY ");
        let s = match sort.unwrap_or(Sort::TaxonSequence) {
            Sort::Id => "sampleid",
            Sort::TaxonName => "complete_name",
            Sort::TaxonSequence => "seq",
            Sort::SourceId => "srcid",
            Sort::SourceName => "srcname",
        };
        builder.push(s);
        builder
    }

    fn build_count(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder: QueryBuilder<Sqlite> =
            QueryBuilder::new("SELECT COUNT(*) as nsamples FROM vsamples");
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder
    }

    pub async fn fetch_all_user(
        userid: i64,
        filter: Option<DynFilterPart>,
        sort: Option<Sort>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Sample>> {
        let mut fbuilder =
            CompoundFilter::build(FilterOp::And).push(Arc::new(Filter::UserId(userid)));
        if let Some(f) = filter {
            fbuilder = fbuilder.push(f);
        }
        let newfilter = fbuilder.build();
        let mut builder = Self::build_query(Some(newfilter), sort);
        Ok(builder.build_query_as().fetch_all(pool).await?)
    }

    pub async fn fetch_all(
        filter: Option<DynFilterPart>,
        sort: Option<Sort>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Sample>> {
        let mut builder = Self::build_query(filter, sort);
        Ok(builder.build_query_as().fetch_all(pool).await?)
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> Result<Sample> {
        let mut builder = Self::build_query(Some(Arc::new(Filter::Id(Cmp::Equal, id))), None);
        Ok(builder.build_query_as().fetch_one(pool).await?)
    }

    pub async fn count(filter: Option<DynFilterPart>, pool: &Pool<Sqlite>) -> Result<i64> {
        let mut builder = Self::build_count(filter);
        builder
            .build()
            .fetch_one(pool)
            .await?
            .try_get("nsamples")
            .map_err(|e| e.into())
    }

    pub async fn insert(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.id != -1 {
            return Err(Error::InvalidOperationObjectAlreadyExists(self.id));
        }
        sqlx::query("INSERT INTO sc_samples (tsn, userid, srcid, month, year, quantity, notes, certainty) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(self.taxon.id())
        .bind(self.user.id())
        .bind(self.source.id())
        .bind(self.month)
        .bind(self.year)
        .bind(self.quantity)
        .bind(&self.notes)
        .bind(&self.certainty)
        .execute(pool)
        .await
        .map(|r| { self.id = r.last_insert_rowid(); r})
        .map_err(|e| e.into())
    }

    pub async fn update(&self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(Error::InvalidOperationObjectNotFound);
        }
        if self.taxon.id() < 0 {
            return Err(Error::InvalidStateMissingAttribute("taxon".to_string()));
        }
        if self.source.id() < 0 {
            return Err(Error::InvalidStateMissingAttribute("source".to_string()));
        }

        sqlx::query("Update sc_samples SET tsn=?, srcid=?, month=?, year=?, quantity=?, notes=?, certainty=? WHERE sampleid=?")
            .bind(self.taxon.id())
            .bind(self.source.id())
            .bind(self.month)
            .bind(self.year)
            .bind(self.quantity)
            .bind(&self.notes)
            .bind(&self.certainty)
            .bind(self.id)
            .execute(pool)
            .await.map_err(|e| e.into())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        taxonid: i64,
        userid: i64,
        sourceid: i64,
        month: Option<u32>,
        year: Option<u32>,
        quantity: Option<i64>,
        notes: Option<String>,
        certainty: Certainty,
    ) -> Self {
        Self {
            id: -1,
            user: ExternalRef::Stub(userid),
            taxon: ExternalRef::Stub(taxonid),
            source: ExternalRef::Stub(sourceid),
            quantity,
            month,
            year,
            notes,
            certainty,
        }
    }
}

impl FromRow<'_, SqliteRow> for Sample {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("sampleid")?,
            user: FromRow::from_row(row)?,
            taxon: FromRow::from_row(row)?,
            source: FromRow::from_row(row)?,
            quantity: row.try_get("quantity").unwrap_or(None),
            month: row.try_get("month").unwrap_or(None),
            year: row.try_get("year").unwrap_or(None),
            notes: row.try_get("notes").unwrap_or(None),
            certainty: row.try_get("certainty").unwrap_or(Certainty::Uncertain),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("users", "sources", "taxa"))
    ))]
    async fn insert_samples(pool: Pool<Sqlite>) {
        async fn check(
            pool: &Pool<Sqlite>,
            taxon: i64,
            user: i64,
            source: i64,
            quantity: Option<i64>,
            month: Option<u32>,
            year: Option<u32>,
            notes: Option<String>,
            certainty: Certainty,
        ) {
            let mut sample =
                Sample::new(taxon, user, source, month, year, quantity, notes, certainty);
            let res = sample.insert(pool).await;
            let res = res.expect("Failed to insert sample");
            let loaded = Sample::load(res.last_insert_rowid(), pool)
                .await
                .expect("Failed to load sample from database");
            assert_eq!(sample.id, loaded.id);
            assert_eq!(sample.user, loaded.user);
            assert_eq!(sample.taxon.id(), loaded.taxon.id());
            assert_eq!(sample.source.id(), loaded.source.id());
            assert_eq!(sample.month, loaded.month);
            assert_eq!(sample.year, loaded.year);
            assert_eq!(sample.quantity, loaded.quantity);
            assert_eq!(sample.notes, loaded.notes);
            assert_eq!(sample.certainty, loaded.certainty);
        }
        check(
            &pool,
            40683,
            1,
            1,
            None,
            None,
            None,
            None,
            Certainty::Uncertain,
        )
        .await;
        check(
            &pool,
            40683,
            1,
            1,
            Some(100),
            Some(12),
            Some(2023),
            Some("these are notes".to_string()),
            Certainty::Certain,
        )
        .await;
    }
}

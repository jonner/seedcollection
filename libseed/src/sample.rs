//! Objects to keep track of samples of seeds that were collected or purchased
use crate::{
    error::{Error, Result},
    loadable::{ExternalRef, Loadable},
    query::{Cmp, CompoundFilter, DynFilterPart, FilterPart, Op, SortSpecs, ToSql},
    source::Source,
    taxonomy::{Rank, Taxon},
    user::User,
    Database,
};
use async_trait::async_trait;
use serde::{de::IntoDeserializer, Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteQueryResult, SqliteRow},
    FromRow, QueryBuilder, Row, Sqlite,
};
use std::{str::FromStr, sync::Arc};
use strum_macros::Display;

/// A representation of the certainty of identification for a sample
#[derive(Clone, Deserialize, Serialize, Debug, sqlx::Type, PartialEq, Display)]
#[repr(i32)]
pub enum Certainty {
    /// ID is certain
    Certain = 1,

    /// ID is uncertain
    Uncertain = 2,
}

impl From<Filter> for DynFilterPart {
    fn from(value: Filter) -> Self {
        Arc::new(value)
    }
}

// FIXME: can we combine this with `SortField`?
/// A type that provides fields that can be used to filter a database query for [Sample]s
#[derive(Clone)]
pub enum Filter {
    /// Compared the sample's ID with the given value
    Id(Cmp, i64),

    /// Matches Samples whose IDs are *not* contained in the given list
    IdNotIn(Vec<i64>),

    /// Compares the ID of a sample's [Source] with the given value
    SourceId(Cmp, i64),

    /// Matches samples whose [Source] name contains the given string
    SourceNameLike(String),

    /// Compares the ID of the sample's [Taxon] with the given value
    TaxonId(Cmp, i64),

    /// Matches samples whose first [Taxon] name (typically genus) contains the given string
    TaxonName1(Cmp, String),

    /// Matches samples whose second [Taxon] name (typically species) contains the given string
    TaxonName2(Cmp, String),

    /// Matches samples whose third [Taxon] name (typically subspecies) contains the given string
    TaxonName3(Cmp, String),

    /// Matches samples whose [Taxon] common name contains the given string
    TaxonCommonName(Cmp, String),

    /// Matches samples whose user ID matches the given value
    UserId(i64),

    /// Compares the sample's note field with the given string value
    Notes(Cmp, String),

    /// Compares the quantity of the sample with the given value
    Quantity(Cmp, i64),
    TaxonRank(Cmp, Rank),
}

pub fn taxon_name_like(substr: String) -> DynFilterPart {
    CompoundFilter::builder(Op::Or)
        .push(Filter::TaxonName1(Cmp::Like, substr.clone()))
        .push(Filter::TaxonName2(Cmp::Like, substr.clone()))
        .push(Filter::TaxonName3(Cmp::Like, substr.clone()))
        .push(Filter::TaxonCommonName(Cmp::Like, substr.clone()))
        .build()
}

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(cmp, id) => {
                match cmp {
                    Cmp::NumericPrefix => builder
                        .push(format!("CAST(sampleid as TEXT)"))
                        .push(cmp)
                        .push(" CONCAT(")
                        .push_bind(*id)
                        .push(",'%')"),
                    _ => builder.push("sampleid").push(cmp).push_bind(*id),
                };
            }
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
            Self::TaxonName1(cmp, s) => {
                if !s.is_empty() {
                    builder.push(" unit_name1 ").push(cmp);
                    match cmp {
                        Cmp::Like => {
                            let wildcard = format!("%{s}%");
                            builder.push_bind(wildcard.clone());
                        }
                        _ => _ = builder.push_bind(s.clone()),
                    }
                }
            }
            Self::TaxonName2(cmp, s) => {
                if !s.is_empty() {
                    builder.push(" unit_name2 ").push(cmp);
                    match cmp {
                        Cmp::Like => {
                            let wildcard = format!("%{s}%");
                            builder.push_bind(wildcard.clone());
                        }
                        _ => _ = builder.push_bind(s.clone()),
                    }
                }
            }
            Self::TaxonName3(cmp, s) => {
                if !s.is_empty() {
                    builder.push(" unit_name3 ").push(cmp);
                    match cmp {
                        Cmp::Like => {
                            let wildcard = format!("%{s}%");
                            builder.push_bind(wildcard.clone());
                        }
                        _ => _ = builder.push_bind(s.clone()),
                    }
                }
            }
            Self::TaxonCommonName(cmp, s) => {
                if !s.is_empty() {
                    builder.push(" cnames ").push(cmp);
                    match cmp {
                        Cmp::Like => {
                            let wildcard = format!("%{s}%");
                            builder.push_bind(wildcard.clone());
                        }
                        _ => _ = builder.push_bind(s.clone()),
                    }
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
            Self::Quantity(cmp, n) => _ = builder.push("quantity").push(cmp).push_bind(*n),
            Self::TaxonRank(cmp, rank) => {
                _ = builder
                    .push("rank")
                    .push(cmp)
                    .push_bind(rank.clone() as i64)
            }
        };
    }
}

// FIXME: can we combine this with `Filter`?
/// A type that provides fields that can be used to sort results of a database query for Samples
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SortField {
    /// Sort by the Sample's ID
    Id,

    /// Sort by the name of the sample's taxon
    #[serde(rename = "name")]
    TaxonName,

    /// Sort by the taxonomic sequence
    #[serde(rename = "seq")]
    TaxonSequence,

    /// Sort by the ID of the sample's Source
    SourceId,

    /// Sort by the name of the sample's Source
    SourceName,

    /// Sort by the date that the sample was collected
    CollectionDate,

    /// Sort by the quantity of the sample
    Quantity,
}

impl FromStr for SortField {
    type Err = serde::de::value::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let deserializer = s.into_deserializer();
        Deserialize::deserialize(deserializer)
    }
}

impl ToSql for SortField {
    fn to_sql(&self) -> String {
        match self {
            SortField::Id => "sampleid",
            SortField::TaxonName => "complete_name",
            SortField::TaxonSequence => "seq",
            SortField::SourceId => "srcid",
            SortField::SourceName => "srcname",
            SortField::CollectionDate => "CONCAT(year, month)",
            SortField::Quantity => "quantity",
        }
        .into()
    }
}

/// An object that represents information about a seed collection sample
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Sample {
    /// A unique ID to represent this sample
    pub id: i64,
    /// The user that is owns this sample
    pub user: ExternalRef<User>,
    /// The taxon associated with this seed sample
    pub taxon: ExternalRef<Taxon>,
    /// The source of this particular seed sample
    pub source: ExternalRef<Source>,
    /// The quantity of seeds that exist in this sample
    pub quantity: Option<i64>,
    /// The month that the sample was acquired or collected
    pub month: Option<u32>,
    /// The year that the sample was acquired or collected
    pub year: Option<u32>,
    /// Free-form notes describing this seed sample
    pub notes: Option<String>,
    /// Indicates whether the taxon assigned to this sample is certain or not
    pub certainty: Certainty,
}

#[async_trait]
impl Loadable for Sample {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id
    }

    async fn load(id: Self::Id, db: &Database) -> Result<Self> {
        let mut builder = Self::build_query(Some(Filter::Id(Cmp::Equal, id).into()), None);
        Ok(builder.build_query_as().fetch_one(db.pool()).await?)
    }

    async fn delete_id(id: &Self::Id, db: &Database) -> Result<SqliteQueryResult> {
        sqlx::query("DELETE FROM sc_samples WHERE sampleid=?")
            .bind(id)
            .execute(db.pool())
            .await
            .map_err(|e| e.into())
    }
}

impl Sample {
    fn build_query(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<SortField>>,
    ) -> QueryBuilder<'static, Sqlite> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT * FROM vsamples");
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" ORDER BY ");
        let s: SortSpecs<SortField> = sort
            .map(Into::into)
            .unwrap_or(SortField::TaxonSequence.into());
        builder.push(s.to_sql());
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

    /// Loads all matching samples from the database for the given user
    pub async fn load_all_user(
        userid: i64,
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<SortField>>,
        db: &Database,
    ) -> Result<Vec<Sample>> {
        let mut fbuilder = CompoundFilter::builder(Op::And).push(Filter::UserId(userid));
        if let Some(f) = filter {
            fbuilder = fbuilder.push(f);
        }
        let newfilter = fbuilder.build();
        let mut builder = Self::build_query(Some(newfilter), sort);
        Ok(builder.build_query_as().fetch_all(db.pool()).await?)
    }

    /// Loads all matching samples from the database
    pub async fn load_all(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<SortField>>,
        db: &Database,
    ) -> Result<Vec<Sample>> {
        let mut builder = Self::build_query(filter, sort);
        Ok(builder.build_query_as().fetch_all(db.pool()).await?)
    }

    /// Queries the count of all matching samples from the database
    pub async fn count(filter: Option<DynFilterPart>, db: &Database) -> Result<i64> {
        let mut builder = Self::build_count(filter);
        builder
            .build()
            .fetch_one(db.pool())
            .await?
            .try_get("nsamples")
            .map_err(|e| e.into())
    }

    /// Add this sample to the database. If this call completes successfully,
    /// the id of this object will be updated to the ID of the inserted row in the
    /// database
    pub async fn insert(&mut self, db: &Database) -> Result<SqliteQueryResult> {
        if self.id != -1 {
            return Err(Error::InvalidInsertObjectAlreadyExists(self.id));
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
        .execute(db.pool())
        .await
        .inspect(|r| self.id = r.last_insert_rowid())
        .map_err(|e| e.into())
    }

    /// Update the sample in the database so that it matches this object
    pub async fn update(&self, db: &Database) -> Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(Error::InvalidUpdateObjectNotFound);
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
            .execute(db.pool())
            .await.map_err(|e| e.into())
    }

    /// Create a new sample with the given data. It iwll initially have an
    /// invalid ID until it is inserted into the database.
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
            id: Self::invalid_id(),
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

impl FromRow<'_, SqliteRow> for ExternalRef<Sample> {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Sample::from_row(row)
            .map(ExternalRef::Object)
            .or_else(|_| row.try_get("sampleid").map(ExternalRef::Stub))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Pool;
    use test_log::test;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("users", "sources", "taxa"))
    ))]
    async fn insert_samples(pool: Pool<Sqlite>) {
        let db = Database(pool);

        async fn check(
            db: &Database,
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
            let res = sample.insert(&db).await;
            let res = res.expect("Failed to insert sample");
            let loaded = Sample::load(res.last_insert_rowid(), db)
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
            &db,
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
            &db,
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

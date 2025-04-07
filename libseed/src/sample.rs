//! Objects to keep track of samples of seeds that were collected or purchased
use crate::{
    core::{
        database::Database,
        error::{Error, Result},
        loadable::{ExternalRef, Loadable},
        query::{
            DynFilterPart, LimitSpec, SortSpecs, ToSql,
            filter::{Cmp, FilterPart, or},
        },
    },
    source::Source,
    taxonomy::{Rank, Taxon},
    user::User,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize, de::IntoDeserializer};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite, sqlite::SqliteRow};
use std::str::FromStr;
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

// FIXME: can we combine this with `SortField`?
/// A type that provides fields that can be used to filter a database query for [Sample]s
#[derive(Clone)]
pub enum Filter {
    /// Compared the sample's ID with the given value
    Id(Cmp, <Sample as Loadable>::Id),

    /// Matches Samples whose IDs are *not* contained in the given list
    IdNotIn(Vec<<Sample as Loadable>::Id>),

    /// Compares the ID of a sample's [Source] with the given value
    SourceId(Cmp, <Source as Loadable>::Id),

    /// Matches samples whose [Source] name contains the given string
    SourceName(Cmp, String),

    /// Compares the ID of the sample's [Taxon] with the given value
    TaxonId(Cmp, <Taxon as Loadable>::Id),

    /// Matches samples whose first [Taxon] name (typically genus) contains the given string
    TaxonName1(Cmp, String),

    /// Matches samples whose second [Taxon] name (typically species) contains the given string
    TaxonName2(Cmp, String),

    /// Matches samples whose third [Taxon] name (typically subspecies) contains the given string
    TaxonName3(Cmp, String),

    /// Matches samples whose [Taxon] common name contains the given string
    TaxonCommonName(Cmp, String),

    /// Matches samples whose user ID matches the given value
    UserId(<User as Loadable>::Id),

    /// Compares the sample's note field with the given string value
    Notes(Cmp, String),

    /// Compares the quantity of the sample with the given value
    Quantity(Cmp, f64),
    TaxonRank(Cmp, Rank),
}

/// Creates a query filter to match any [Sample] object when any of the
/// components of the taxon name matches the given `substr`
pub fn taxon_name_like(substr: &str) -> DynFilterPart {
    or().push(Filter::TaxonName1(Cmp::Like, substr.into()))
        .push(Filter::TaxonName2(Cmp::Like, substr.into()))
        .push(Filter::TaxonName3(Cmp::Like, substr.into()))
        .push(Filter::TaxonCommonName(Cmp::Like, substr.into()))
        .build()
}

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(cmp, id) => {
                match cmp {
                    Cmp::NumericPrefix => builder
                        .push("CAST(sampleid as TEXT)".to_string())
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
                            builder
                                .push("CONCAT('%',")
                                .push_bind(s.clone())
                                .push(",'%')");
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
                            builder
                                .push("CONCAT('%',")
                                .push_bind(s.clone())
                                .push(",'%')");
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
                            builder
                                .push("CONCAT('%',")
                                .push_bind(s.clone())
                                .push(",'%')");
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
                            builder
                                .push("CONCAT('%',")
                                .push_bind(s.clone())
                                .push(",'%')");
                        }
                        _ => _ = builder.push_bind(s.clone()),
                    }
                }
            }
            Self::UserId(id) => _ = builder.push("userid=").push_bind(*id),
            Self::Notes(cmp, s) => {
                _ = builder.push("notes").push(cmp);
                match cmp {
                    Cmp::Like => builder
                        .push("CONCAT('%',")
                        .push_bind(s.clone())
                        .push(", '%')"),
                    _ => builder.push_bind(s.clone()),
                };
            }
            Self::SourceName(cmp, s) => {
                if !s.is_empty() {
                    builder.push("srcname").push(cmp);
                    match cmp {
                        Cmp::Like => builder
                            .push("CONCAT('%',")
                            .push_bind(s.clone())
                            .push(",'%')"),
                        _ => builder.push_bind(s.clone()),
                    };
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

/// A type for holding statistics about the samples in the seed collection
#[derive(FromRow, Serialize)]
pub struct SampleStats {
    /// The number of total samples in teh collection
    pub nsamples: i64,
    /// The number of unique taxa represented by the samples in the collection
    pub ntaxa: i64,
    /// The number of unique sources associated with the samples in the collectiohn
    pub nsources: i64,
}

/// An object that represents information about a seed collection sample
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Sample {
    /// A unique ID to represent this sample
    pub id: <Sample as Loadable>::Id,
    /// The user that is owns this sample
    pub user: ExternalRef<User>,
    /// The taxon associated with this seed sample
    pub taxon: ExternalRef<Taxon>,
    /// The source of this particular seed sample
    pub source: ExternalRef<Source>,
    /// The quantity of seeds that exist in this sample
    pub quantity: Option<f64>,
    /// The month that the sample was acquired or collected
    pub month: Option<u8>,
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
    type Sort = SortField;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_invalid(&mut self) {
        self.id = Self::invalid_id()
    }

    async fn insert(&mut self, db: &Database) -> Result<&Self::Id> {
        if self.exists() {
            return Err(Error::InvalidInsertObjectAlreadyExists(self.id));
        }
        let newval = sqlx::query_as(
            "INSERT INTO sc_samples
                (tsn, userid, srcid, month, year, quantity, notes, certainty)
            VALUES
                (?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING *",
        )
        .bind(self.taxon.id())
        .bind(self.user.id())
        .bind(self.source.id())
        .bind(self.month)
        .bind(self.year)
        .bind(self.quantity)
        .bind(&self.notes)
        .bind(&self.certainty)
        .fetch_one(db.pool())
        .await?;
        // FIXME: this will invalidate any of the external refs we had already loaded (e.g. taxon, user, source)
        *self = newval;
        Ok(&self.id)
    }

    async fn load(id: Self::Id, db: &Database) -> Result<Self> {
        let mut builder = Self::query_builder(Some(Filter::Id(Cmp::Equal, id).into()), None, None);
        Ok(builder.build_query_as().fetch_one(db.pool()).await?)
    }

    async fn load_all(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<Self::Sort>>,
        limit: Option<LimitSpec>,
        db: &Database,
    ) -> Result<Vec<Self>> {
        let mut builder = Self::query_builder(filter, sort, limit);
        Ok(builder.build_query_as().fetch_all(db.pool()).await?)
    }

    async fn delete_id(id: &Self::Id, db: &Database) -> Result<()> {
        sqlx::query("DELETE FROM sc_samples WHERE sampleid=?")
            .bind(id)
            .execute(db.pool())
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }

    async fn update(&self, db: &Database) -> Result<()> {
        if !self.exists() {
            return Err(Error::InvalidUpdateObjectNotFound);
        }
        if self.taxon.id() == Taxon::invalid_id() {
            return Err(Error::InvalidStateMissingAttribute("taxon".to_string()));
        }
        if self.source.id() == Source::invalid_id() {
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
            .await?;
        Ok(())
    }
}

impl Sample {
    fn query_builder(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<SortField>>,
        limit: Option<LimitSpec>,
    ) -> QueryBuilder<'static, Sqlite> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT * FROM vsamples");
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        let s: SortSpecs<SortField> = sort.unwrap_or(SortField::TaxonSequence.into());
        builder.push(s.to_sql());
        if let Some(l) = limit {
            builder.push(l.to_sql());
        }
        builder
    }

    fn stats_query_builder(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            "SELECT COUNT(*) as nsamples, COUNT(DISTINCT tsn) as ntaxa, COUNT(DISTINCT srcid) as nsources FROM vsamples",
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder
    }

    /// Queries the count of all matching samples from the database
    pub async fn count(filter: Option<DynFilterPart>, db: &Database) -> Result<i64> {
        let mut builder = Self::stats_query_builder(filter);
        builder
            .build()
            .fetch_one(db.pool())
            .await?
            .try_get("nsamples")
            .map_err(|e| e.into())
    }

    /// Queries the count of all matching samples from the database
    pub async fn stats(filter: Option<DynFilterPart>, db: &Database) -> Result<SampleStats> {
        let mut builder = Self::stats_query_builder(filter);
        builder
            .build_query_as()
            .fetch_one(db.pool())
            .await
            .map_err(Into::into)
    }

    /// Create a new sample with the given data. It iwll initially have an
    /// invalid ID until it is inserted into the database.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        taxonid: <Taxon as Loadable>::Id,
        userid: <User as Loadable>::Id,
        sourceid: <Source as Loadable>::Id,
        month: Option<u8>,
        year: Option<u32>,
        quantity: Option<f64>,
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

    pub fn taxon_display_name(&self) -> Result<String> {
        let mut name = self.taxon.object()?.complete_name.clone();
        if self.certainty == Certainty::Uncertain {
            name += "(?)";
        }
        Ok(name)
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
        let db = Database::from(pool);

        #[allow(clippy::too_many_arguments)]
        async fn check(
            db: &Database,
            taxon: <Taxon as Loadable>::Id,
            user: <User as Loadable>::Id,
            source: <Source as Loadable>::Id,
            quantity: Option<f64>,
            month: Option<u8>,
            year: Option<u32>,
            notes: Option<String>,
            certainty: Certainty,
        ) {
            let mut sample =
                Sample::new(taxon, user, source, month, year, quantity, notes, certainty);
            let id = sample.insert(db).await.expect("Failed to insert sample");
            let loaded = Sample::load(*id, db)
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
            Some(100.0),
            Some(12),
            Some(2023),
            Some("these are notes".to_string()),
            Certainty::Certain,
        )
        .await;
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../db/fixtures",
            scripts("users", "sources", "taxa", "samples")
        )
    ))]
    async fn update_samples(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        let mut sample1 = Sample::load(4, &db).await.expect("Failed to load sample 4");

        assert_eq!(sample1.id, 4);
        assert_eq!(sample1.taxon.id(), 40683);
        assert_eq!(sample1.month, Some(11));
        assert_eq!(sample1.year, Some(2023));
        assert_eq!(sample1.source.id(), 1);
        assert_eq!(sample1.user.id(), 2);
        // save external refs to make sure that they remain equivalent after update
        let taxon = sample1.taxon.clone();
        let src = sample1.source.clone();
        let user = sample1.user.clone();
        println!("{sample1:?}");

        sample1.month = Some(12);
        sample1.update(&db).await.expect("Failed to update sample1");

        assert_eq!(sample1.id, 4);
        assert_eq!(sample1.taxon.id(), 40683);
        assert_eq!(sample1.taxon, taxon);
        assert_eq!(sample1.month, Some(12));
        assert_eq!(sample1.year, Some(2023));
        assert_eq!(sample1.source.id(), 1);
        assert_eq!(sample1.source, src);
        assert_eq!(sample1.user.id(), 2);
        assert_eq!(sample1.user, user);

        println!("{sample1:?}");
    }
}

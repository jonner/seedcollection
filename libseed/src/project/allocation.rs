//! Samples that are allocated to a [Project]
use super::{
    Project,
    note::{self, Note},
};
use crate::{
    Error,
    core::{
        database::Database,
        error::Result,
        loadable::Loadable,
        query::{
            DynFilterPart, LimitSpec, SortOrder, SortSpec, SortSpecs, ToSql,
            filter::{Cmp, FilterPart, or},
        },
    },
    sample::Sample,
    user::User,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite, prelude::*, sqlite::SqliteRow};

// FIXME: Can we combine SortField and Filter somehow???
/// A type to specify a field that can be used to filter allocation objects when
/// querying the database
#[derive(Clone)]
pub enum Filter {
    /// Filter by the allocation ID (NOTE: this is different than the sample ID)
    Id(<AllocatedSample as Loadable>::Id),

    /// Filter based on the user ID of the sample
    UserId(<User as Loadable>::Id),

    /// Filter based on the ID of the project that the sample is allocated to
    ProjectId(<Project as Loadable>::Id),

    /// Filter based on the ID of the sample
    SampleId(<Sample as Loadable>::Id),

    /// Filter for samples whose first taxon name (often genus) matches the given string
    TaxonName1(Cmp, String),

    /// Filter for samples whose second taxon name (often species) matches the given string
    TaxonName2(Cmp, String),

    /// Filter for samples whose third taxon name (often subspecies) matches the given string
    TaxonName3(Cmp, String),

    /// Filter for samples whose taxon common name matches the given string
    TaxonCommonName(Cmp, String),

    /// Filter based on the name of the source of the sample
    SourceName(Cmp, String),

    /// Filter if the sample notes match the given string
    Notes(Cmp, String),
}

/// Creates a query filter to match an [AllocatedSample] object when the any of the
/// components of the sample's taxon name matches the given `substr`
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
            Self::Id(id) => _ = builder.push(" PS.psid = ").push_bind(*id),
            Self::UserId(id) => _ = builder.push(" S.userid = ").push_bind(*id),
            Self::ProjectId(id) => _ = builder.push(" PS.projectid = ").push_bind(*id),
            Self::SampleId(id) => _ = builder.push(" PS.sampleid = ").push_bind(*id),
            Self::TaxonName1(cmp, s) => {
                if !s.is_empty() {
                    builder.push(" unit_name1 ").push(cmp);
                    match cmp {
                        Cmp::Like => {
                            let wildcard = format!("%{s}%");
                            builder.push_bind(wildcard)
                        }
                        _ => builder.push_bind(s.clone()),
                    };
                }
            }
            Self::TaxonName2(cmp, s) => {
                if !s.is_empty() {
                    builder.push(" unit_name2 ").push(cmp);
                    match cmp {
                        Cmp::Like => {
                            let wildcard = format!("%{s}%");
                            builder.push_bind(wildcard)
                        }
                        _ => builder.push_bind(s.clone()),
                    };
                }
            }
            Self::TaxonName3(cmp, s) => {
                if !s.is_empty() {
                    builder.push(" unit_name3 ").push(cmp);
                    match cmp {
                        Cmp::Like => {
                            let wildcard = format!("%{s}%");
                            builder.push_bind(wildcard)
                        }
                        _ => builder.push_bind(s.clone()),
                    };
                }
            }
            Self::TaxonCommonName(cmp, s) => {
                if !s.is_empty() {
                    builder.push(" cnames ").push(cmp);
                    match cmp {
                        Cmp::Like => {
                            let wildcard = format!("%{s}%");
                            builder.push_bind(wildcard)
                        }
                        _ => builder.push_bind(s.clone()),
                    };
                }
            }
            Self::SourceName(cmp, s) => {
                let s = match cmp {
                    Cmp::Like => format!("%{s}%"),
                    _ => s.to_string(),
                };
                builder.push(" S.srcname ").push(cmp).push_bind(s);
            }
            Self::Notes(cmp, s) => _ = builder.push("notes").push(cmp).push_bind(format!("%{s}%")),
        }
    }
}

/// An object representing a [Sample] that has been allocated to a particular [Project]
#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct AllocatedSample {
    /// A unique ID representing this allocation in the database
    pub id: <AllocatedSample as Loadable>::Id,

    /// The Sample associated with this allocation
    pub sample: Sample,

    /// The project that the sample is allocated to
    pub projectid: <Project as Loadable>::Id,

    /// Project-specific notes for this allocation. This can be used to track
    /// status of this sample within the project, etc. For example, has the sample been
    /// planted? Germinated?, etc.
    pub notes: Vec<Note>,
}

#[async_trait]
impl Loadable for AllocatedSample {
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
            return Err(Error::InvalidInsertObjectAlreadyExists(self.id()));
        }
        let newval = sqlx::query_as(
            "INSERT INTO sc_project_samples
                (projectid, sampleid)
            VALUES
                (?, ?)
            RETURNING *",
        )
        .bind(self.projectid)
        .bind(self.sample.id)
        .fetch_one(db.pool())
        .await?;
        *self = newval;
        Ok(&self.id)
    }

    async fn load(id: Self::Id, db: &Database) -> Result<Self> {
        let mut builder = Self::query_builder(Some(Filter::Id(id).into()), None, None);
        Ok(builder.build_query_as().fetch_one(db.pool()).await?)
    }

    async fn load_all(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<SortField>>,
        limit: Option<LimitSpec>,
        db: &Database,
    ) -> Result<Vec<Self>> {
        Self::query_builder(filter, sort, limit)
            .build_query_as()
            .fetch_all(db.pool())
            .await
            .map_err(Into::into)
    }

    async fn count(filter: Option<DynFilterPart>, db: &Database) -> Result<u64> {
        Self::count_query_builder(filter)
            .build()
            .fetch_one(db.pool())
            .await?
            .try_get("count")
            .map_err(Into::into)
    }

    async fn delete_id(id: &Self::Id, db: &Database) -> Result<()> {
        sqlx::query!("DELETE FROM sc_project_samples WHERE psid=?", id)
            .execute(db.pool())
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }

    async fn update(&self, _db: &Database) -> Result<()> {
        return Err(Error::InvalidOperation(
            "Cannot update an allocation".into(),
        ));
    }
}

// FIXME: Can we combine SortField and Filter somehow???
/// A Type to specify a field that will be used to sort the query
#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum SortField {
    /// Sort results according to taxonomic order
    Taxon,

    /// Sort resutls according to sample ID
    #[serde(rename = "id")]
    SampleId,

    /// Sort results according to the date that the sample was collected
    #[serde(rename = "date")]
    CollectionDate,

    /// Sort results by the latest activity (i.e. notes) on this sample
    Activity,

    /// Sort results by the quantity of the sample
    #[serde(rename = "qty")]
    Quantity,

    /// Sort results by the source name of the sample
    #[serde(rename = "src")]
    Source,
}

impl ToSql for SortField {
    fn to_sql(&self) -> String {
        match self {
            SortField::Taxon => "seq",
            SortField::SampleId => "S.sampleid",
            SortField::CollectionDate => "CONCAT(S.year, S.month)",
            SortField::Activity => "N.notedate",
            SortField::Quantity => "S.quantity",
            SortField::Source => " S.srcname",
        }
        .into()
    }
}

impl AllocatedSample {
    fn base_query_builder(
        select_fields: &Vec<&str>,
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<SortField>>,
        limit: Option<LimitSpec>,
    ) -> QueryBuilder<'static, Sqlite> {
        let fields = select_fields.join(", ");
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT ");
        builder.push(fields);
        builder.push(" FROM sc_project_samples PS
            INNER JOIN vsamples S ON PS.sampleid=S.sampleid
            LEFT JOIN ( SELECT * FROM
            (SELECT *, ROW_NUMBER() OVER (PARTITION BY psid ORDER BY DATE(notedate) DESC, pnoteid DESC) AS rownr
            FROM sc_project_notes ORDER BY pnoteid DESC)
            WHERE rownr = 1) N ON N.psid = PS.psid");
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        if let Some(sort) = sort {
            builder.push(sort.to_sql());
        }
        if let Some(l) = limit {
            builder.push(l.to_sql());
        }

        builder
    }

    fn query_builder(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<SortField>>,
        limit: Option<LimitSpec>,
    ) -> QueryBuilder<'static, Sqlite> {
        let sort = sort.or(Some(
            SortSpec::new(SortField::Taxon, SortOrder::Ascending).into(),
        ));
        Self::base_query_builder(
            &vec![
                "PS.psid",
                "PS.projectid",
                "S.*",
                "N.pnoteid",
                "N.notedate",
                "N.notetype",
                "N.notesummary",
                "N.notedetails",
            ],
            filter,
            sort,
            limit,
        )
    }

    fn count_query_builder(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        Self::base_query_builder(&vec!["COUNT(*) as count"], filter, None, None)
    }

    /// Load a single matching [AllocatedSample] from the database. Note that this is
    /// only useful when the `filter` that is specified will return a single result. For
    /// example if you're filtering by [Filter::Id]
    pub async fn load_one(filter: Option<DynFilterPart>, db: &Database) -> sqlx::Result<Self> {
        Self::query_builder(filter, None, None)
            .build_query_as()
            .fetch_one(db.pool())
            .await
    }

    /// Load all notes associated with this allocation
    pub async fn load_notes(&mut self, db: &Database) -> Result<()> {
        self.notes = Note::load_all(
            Some(note::NoteFilter::AllocationId(self.id).into()),
            None,
            None,
            db,
        )
        .await?;
        Ok(())
    }
}

impl FromRow<'_, SqliteRow> for AllocatedSample {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        // querying for allocation will try to return the latest note if any exist
        let mut notes = Vec::new();
        if let Ok(n) = Note::from_row(row) {
            notes.push(n);
        }
        Ok(Self {
            id: row.try_get("psid")?,
            sample: Sample::from_row(row)?,
            projectid: row.try_get("projectid")?,
            notes,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::core::database::Database;

    use super::*;
    use sqlx::Pool;
    use test_log::test;
    use time::Month;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../../db/fixtures",
            scripts("users", "sources", "taxa", "assigned-samples")
        )
    ))]
    async fn load_allocations(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        async fn check_sample(a: &AllocatedSample, db: &Database) {
            tracing::debug!("loading sample");
            let s = Sample::load(a.sample.id(), db)
                .await
                .expect("Failed to load sample");
            assert_eq!(a.sample, s);

            let c = Project::load(a.projectid, db)
                .await
                .expect("Failed to load project");
            assert_eq!(a.projectid, c.id());
        }

        // check allocations for project 1
        let assigned =
            AllocatedSample::load_all(Some(Filter::ProjectId(1).into()), None, None, &db)
                .await
                .expect("Failed to load assigned samples for first project");

        assert_eq!(assigned.len(), 2);

        tracing::debug!("{:?}", assigned[0]);
        assert_eq!(assigned[0].sample.id(), 1);
        assert_eq!(assigned[0].projectid, 1);
        // querying allocations should also load the latest note
        assert_eq!(assigned[0].notes.len(), 1);
        assert_eq!(assigned[0].notes[0].id, 2);
        assert_eq!(assigned[0].notes[0].date.year(), 2023);
        assert_eq!(assigned[0].notes[0].date.month(), Month::December);
        assert_eq!(assigned[0].notes[0].date.day(), 27);
        assert_eq!(assigned[0].notes[0].summary, "Note summary 2");
        assert_eq!(
            assigned[0].notes[0].details,
            Some("note details 2".to_string())
        );
        check_sample(&assigned[0], &db).await;

        tracing::debug!("{:?}", assigned[1]);
        assert_eq!(assigned[1].sample.id(), 2);
        assert_eq!(assigned[1].projectid, 1);
        check_sample(&assigned[1], &db).await;

        // check allocations for project 2
        let assigned =
            AllocatedSample::load_all(Some(Filter::ProjectId(2).into()), None, None, &db)
                .await
                .expect("Failed to load assigned samples for first project");

        assert_eq!(assigned.len(), 2);

        assert_eq!(assigned[0].sample.id(), 1);
        assert_eq!(assigned[0].projectid, 2);
        check_sample(&assigned[0], &db).await;

        assert_eq!(assigned[1].sample.id(), 3);
        assert_eq!(assigned[1].projectid, 2);
        check_sample(&assigned[1], &db).await;

        // check allocations for sample 1
        let assigned = AllocatedSample::load_all(Some(Filter::SampleId(1).into()), None, None, &db)
            .await
            .expect("Failed to load assigned samples for first project");

        assert_eq!(assigned.len(), 2);

        assert_eq!(assigned[0].sample.id(), 1);
        assert_eq!(assigned[0].projectid, 1);
        check_sample(&assigned[0], &db).await;

        assert_eq!(assigned[1].sample.id(), 1);
        assert_eq!(assigned[1].projectid, 2);
        check_sample(&assigned[1], &db).await;
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../../db/fixtures",
            scripts("users", "sources", "taxa", "assigned-samples")
        )
    ))]
    async fn count_allocations(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        let nallocs = AllocatedSample::count(None, &db)
            .await
            .expect("Failed to count allocations");
        assert_eq!(4, nallocs);
        let nallocs = AllocatedSample::count(Some(Filter::SampleId(1).into()), &db)
            .await
            .expect("Failed to count allocations");
        assert_eq!(2, nallocs);
        AllocatedSample::delete_id(&4, &db)
            .await
            .expect("Failed to delete sample");
        let nallocs = AllocatedSample::count(None, &db)
            .await
            .expect("Failed to count allocations");
        assert_eq!(3, nallocs);
        let nallocs = AllocatedSample::count(Some(Filter::SampleId(1).into()), &db)
            .await
            .expect("Failed to count allocations");
        assert_eq!(1, nallocs);
    }
}

//! Manage samples that are allocated to a [Project]
use super::{
    note::{self, Note},
    Project,
};
use crate::{
    error::Result,
    loadable::Loadable,
    query::{
        Cmp, CompoundFilter, DynFilterPart, FilterPart, Op, SortOrder, SortSpec, SortSpecs, ToSql,
    },
    sample::Sample,
    Database,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{
    prelude::*,
    sqlite::{SqliteQueryResult, SqliteRow},
    QueryBuilder, Sqlite,
};
use std::sync::Arc;

impl From<Filter> for DynFilterPart {
    fn from(value: Filter) -> Self {
        Arc::new(value)
    }
}

// FIXME: Can we combine SortField and Filter somehow???
/// A type to specify a field that can be used to filter allocation objects when
/// querying the database
#[derive(Clone)]
pub enum Filter {
    /// Filter by the allocation ID (NOTE: this is different than the sample ID)
    Id(i64),

    /// Filter based on the user ID of the sample
    UserId(i64),

    /// Filter based on the ID of the project that the sample is allocated to
    ProjectId(i64),

    /// Filter based on the ID of the sample
    SampleId(i64),

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
pub struct Allocation {
    /// A unique ID representing this allocation in the database
    pub id: i64,

    /// The Sample associated with this allocation
    pub sample: Sample,

    /// The project that the sample is allocated to
    pub project: Project,

    /// Project-specific notes for this allocation. This can be used to track
    /// status of this sample within the project, etc. For example, has the sample been
    /// planted? Germinated?, etc.
    pub notes: Vec<Note>,
}

#[async_trait]
impl Loadable for Allocation {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id
    }

    async fn load(id: Self::Id, db: &Database) -> Result<Self> {
        let mut builder = Self::build_query(Some(Filter::Id(id).into()), None);
        Ok(builder.build_query_as().fetch_one(db.pool()).await?)
    }

    async fn delete_id(id: &Self::Id, db: &Database) -> Result<SqliteQueryResult> {
        sqlx::query!("DELETE FROM sc_project_samples WHERE psid=?", id)
            .execute(db.pool())
            .await
            .map_err(|e| e.into())
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

impl Allocation {
    fn build_query(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<SortField>>,
    ) -> QueryBuilder<'static, Sqlite> {
        let sort = sort.unwrap_or(SortSpec::new(SortField::Taxon, SortOrder::Ascending).into());
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            r#"
            SELECT PS.psid,
            S.*,
            P.projectid, P.projname, P.projdescription,
            N.pnoteid, N.notedate, N.notetype, N.notesummary, N.notedetails

            FROM sc_project_samples PS
            INNER JOIN vsamples S ON PS.sampleid=S.sampleid
            INNER JOIN sc_projects P on P.projectid=PS.projectid
            LEFT JOIN ( SELECT * FROM
            (SELECT *, ROW_NUMBER() OVER (PARTITION BY psid ORDER BY DATE(notedate) DESC, pnoteid DESC) AS rownr
            FROM sc_project_notes ORDER BY pnoteid DESC)
            WHERE rownr = 1) N ON N.psid = PS.psid
            "#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" ORDER BY ");
        builder.push(sort.to_sql());

        builder
    }

    /// Load all matching [Allocation]s from the database
    pub async fn load_all(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<SortField>>,
        db: &Database,
    ) -> Result<Vec<Self>, sqlx::Error> {
        Self::build_query(filter, sort)
            .build_query_as()
            .fetch_all(db.pool())
            .await
    }

    /// Load a single matching [Allocation] from the database. Note that this is
    /// only useful when the `filter` that is specified will return a single result. For
    /// example if you're filtering by [Filter::Id]
    pub async fn load_one(
        filter: Option<DynFilterPart>,
        db: &Database,
    ) -> Result<Self, sqlx::Error> {
        Self::build_query(filter, None)
            .build_query_as()
            .fetch_one(db.pool())
            .await
    }

    /// Load all notes associated with this allocation
    pub async fn load_notes(&mut self, db: &Database) -> Result<()> {
        self.notes =
            Note::load_all(Some(Arc::new(note::NoteFilter::AllocationId(self.id))), db).await?;
        Ok(())
    }
}

impl FromRow<'_, SqliteRow> for Allocation {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        // querying for allocation will try to return the latest note if any exist
        let mut notes = Vec::new();
        if let Ok(n) = Note::from_row(row) {
            notes.push(n);
        }
        Ok(Self {
            id: row.try_get("psid")?,
            sample: Sample::from_row(row)?,
            project: Project::from_row(row)?,
            notes,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::Database;

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
        let db = Database(pool);
        async fn check_sample(a: &Allocation, db: &Database) {
            tracing::debug!("loading sample");
            let s = Sample::load(a.sample.id(), db)
                .await
                .expect("Failed to load sample");
            assert_eq!(a.sample, s);

            let c = Project::load(a.project.id(), db)
                .await
                .expect("Failed to load project");
            assert_eq!(a.project, c);
        }

        // check allocations for project 1
        let assigned = Allocation::load_all(Some(Arc::new(Filter::ProjectId(1))), None, &db)
            .await
            .expect("Failed to load assigned samples for first project");

        assert_eq!(assigned.len(), 2);

        tracing::debug!("{:?}", assigned[0]);
        assert_eq!(assigned[0].sample.id(), 1);
        assert_eq!(assigned[0].project.id(), 1);
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
        assert_eq!(assigned[1].project.id(), 1);
        check_sample(&assigned[1], &db).await;

        // check allocations for project 2
        let assigned = Allocation::load_all(Some(Arc::new(Filter::ProjectId(2))), None, &db)
            .await
            .expect("Failed to load assigned samples for first project");

        assert_eq!(assigned.len(), 2);

        assert_eq!(assigned[0].sample.id(), 1);
        assert_eq!(assigned[0].project.id(), 2);
        check_sample(&assigned[0], &db).await;

        assert_eq!(assigned[1].sample.id(), 3);
        assert_eq!(assigned[1].project.id(), 2);
        check_sample(&assigned[1], &db).await;

        // check allocations for sample 1
        let assigned = Allocation::load_all(Some(Arc::new(Filter::SampleId(1))), None, &db)
            .await
            .expect("Failed to load assigned samples for first project");

        assert_eq!(assigned.len(), 2);

        assert_eq!(assigned[0].sample.id(), 1);
        assert_eq!(assigned[0].project.id(), 1);
        check_sample(&assigned[0], &db).await;

        assert_eq!(assigned[1].sample.id(), 1);
        assert_eq!(assigned[1].project.id(), 2);
        check_sample(&assigned[1], &db).await;
    }
}

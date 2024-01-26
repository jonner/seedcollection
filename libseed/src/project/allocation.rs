use super::{
    note::{self, Note},
    Project,
};
use crate::{
    error::Result,
    filter::{DynFilterPart, FilterPart, SortOrder, SortSpec},
    loadable::Loadable,
    sample::Sample,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{
    prelude::*,
    sqlite::{SqliteQueryResult, SqliteRow},
    Pool, QueryBuilder, Sqlite,
};
use std::sync::Arc;

#[derive(Clone)]
pub enum AllocationFilter {
    Id(i64),
    User(i64),
    Project(i64),
    Sample(i64),
}

impl FilterPart for AllocationFilter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(id) => _ = builder.push(" PS.psid = ").push_bind(*id),
            Self::User(id) => _ = builder.push(" S.userid = ").push_bind(*id),
            Self::Project(id) => _ = builder.push(" PS.projectid = ").push_bind(*id),
            Self::Sample(id) => _ = builder.push(" PS.sampleid = ").push_bind(*id),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Allocation {
    pub id: i64,
    pub sample: Sample,
    pub project: Project,
    pub notes: Vec<Note>,
}

#[async_trait]
impl Loadable for Allocation {
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
        Allocation::fetch(id, pool).await
    }

    async fn delete_id(id: &Self::Id, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        sqlx::query!("DELETE FROM sc_project_samples WHERE psid=?", id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
    }
}

#[derive(strum_macros::Display, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum SortField {
    Taxon,
    #[serde(rename = "id")]
    SampleId,
    #[serde(rename = "date")]
    CollectionDate,
    Activity,
    #[serde(rename = "qty")]
    Quantity,
    #[serde(rename = "src")]
    Source,
}

impl Allocation {
    pub fn build_query(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpec<SortField>>,
    ) -> QueryBuilder<'static, Sqlite> {
        let sort = sort.unwrap_or(SortSpec::new(SortField::Taxon, SortOrder::Ascending));
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

        match sort.field {
            SortField::SampleId => _ = builder.push(" S.sampleid"),
            SortField::Taxon => _ = builder.push(" seq"),
            SortField::Activity => _ = builder.push(" N.notedate"),
            SortField::Quantity => _ = builder.push(" S.quantity"),
            SortField::Source => _ = builder.push(" S.srcname"),
            SortField::CollectionDate => _ = builder.push(" CONCAT(S.year, S.month)"),
        }

        match sort.order {
            SortOrder::Ascending => _ = builder.push(" ASC"),
            SortOrder::Descending => _ = builder.push(" DESC"),
        }

        builder
    }

    pub async fn fetch_all(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpec<SortField>>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        Self::build_query(filter, sort)
            .build_query_as()
            .fetch_all(pool)
            .await
    }

    pub async fn fetch_one(
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> Result<Self, sqlx::Error> {
        Self::build_query(filter, None)
            .build_query_as()
            .fetch_one(pool)
            .await
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> Result<Self> {
        let mut builder = Self::build_query(Some(Arc::new(AllocationFilter::Id(id))), None);
        Ok(builder.build_query_as().fetch_one(pool).await?)
    }

    pub async fn fetch_notes(&mut self, pool: &Pool<Sqlite>) -> Result<()> {
        self.notes = Note::fetch_all(
            Some(Arc::new(note::NoteFilter::AllocationId(self.id))),
            pool,
        )
        .await?;
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
    use super::*;
    use sqlx::{Pool, Sqlite};
    use test_log::test;
    use time::Month;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../../db/fixtures",
            scripts("users", "sources", "taxa", "assigned-samples")
        )
    ))]
    async fn fetch_allocations(pool: Pool<Sqlite>) {
        async fn check_sample(a: &Allocation, pool: &Pool<Sqlite>) {
            tracing::debug!("loading sample");
            let s = Sample::load(a.sample.id, pool)
                .await
                .expect("Failed to load sample");
            assert_eq!(a.sample, s);

            let c = Project::load(a.project.id, pool)
                .await
                .expect("Failed to load project");
            assert_eq!(a.project, c);
        }

        // check allocations for project 1
        let assigned =
            Allocation::fetch_all(Some(Arc::new(AllocationFilter::Project(1))), None, &pool)
                .await
                .expect("Failed to load assigned samples for first project");

        assert_eq!(assigned.len(), 2);

        tracing::debug!("{:?}", assigned[0]);
        assert_eq!(assigned[0].sample.id, 1);
        assert_eq!(assigned[0].project.id, 1);
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
        check_sample(&assigned[0], &pool).await;

        tracing::debug!("{:?}", assigned[1]);
        assert_eq!(assigned[1].sample.id, 2);
        assert_eq!(assigned[1].project.id, 1);
        check_sample(&assigned[1], &pool).await;

        // check allocations for project 2
        let assigned =
            Allocation::fetch_all(Some(Arc::new(AllocationFilter::Project(2))), None, &pool)
                .await
                .expect("Failed to load assigned samples for first project");

        assert_eq!(assigned.len(), 2);

        assert_eq!(assigned[0].sample.id, 1);
        assert_eq!(assigned[0].project.id, 2);
        check_sample(&assigned[0], &pool).await;

        assert_eq!(assigned[1].sample.id, 3);
        assert_eq!(assigned[1].project.id, 2);
        check_sample(&assigned[1], &pool).await;

        // check allocations for sample 1
        let assigned =
            Allocation::fetch_all(Some(Arc::new(AllocationFilter::Sample(1))), None, &pool)
                .await
                .expect("Failed to load assigned samples for first project");

        assert_eq!(assigned.len(), 2);

        assert_eq!(assigned[0].sample.id, 1);
        assert_eq!(assigned[0].project.id, 1);
        check_sample(&assigned[0], &pool).await;

        assert_eq!(assigned[1].sample.id, 1);
        assert_eq!(assigned[1].project.id, 2);
        check_sample(&assigned[1], &pool).await;
    }
}

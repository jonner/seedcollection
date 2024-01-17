use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{prelude::*, sqlite::SqliteRow, Pool, QueryBuilder, Sqlite};

use crate::{
    error::Result,
    filter::{DynFilterPart, FilterPart},
    loadable::Loadable,
    sample::Sample,
};

use super::{
    note::{self, Note},
    Collection,
};

#[derive(Clone)]
pub enum AllocationFilter {
    Id(i64),
    User(i64),
    Collection(i64),
    Sample(i64),
}

impl FilterPart for AllocationFilter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(id) => _ = builder.push(" CS.csid = ").push_bind(*id),
            Self::User(id) => _ = builder.push(" S.userid = ").push_bind(*id),
            Self::Collection(id) => _ = builder.push(" CS.collectionid = ").push_bind(*id),
            Self::Sample(id) => _ = builder.push(" CS.sampleid = ").push_bind(*id),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct Allocation {
    pub id: i64,
    pub sample: Sample,
    pub collection: Collection,
    pub notes: Vec<Note>,
    pub loaded: bool,
}

impl Default for Allocation {
    fn default() -> Self {
        Self {
            id: -1,
            sample: Default::default(),
            collection: Default::default(),
            notes: Default::default(),
            loaded: false,
        }
    }
}

#[async_trait]
impl Loadable for Allocation {
    type Id = i64;

    fn new_loadable(id: Self::Id) -> Self {
        let mut a: Allocation = Default::default();
        a.id = id;
        a
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn is_loadable(&self) -> bool {
        self.id > 0
    }

    async fn do_load(&mut self, pool: &Pool<Sqlite>) -> Result<Self> {
        Allocation::fetch(self.id, pool).await.and_then(|mut a| {
            a.loaded = true;
            Ok(a)
        })
    }
}

impl Allocation {
    pub fn build_query(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            r#"
            SELECT CS.csid,

            S.sampleid, quantity, month, year, notes, certainty,

            T.tsn, T.parent_tsn as parentid,
            T.unit_name1, T.unit_name2, T.unit_name3, T.phylo_sort_seq as seq,
            GROUP_CONCAT(V.vernacular_name, "@") as cnames,

            L.locid, L.name as locname, L.description as locdescription, T.complete_name,

            S.userid, U.username,

            C.collectionid, C.name, C.description,
            N.csnoteid, N.date, N.kind, N.summary, N.details

            FROM sc_collection_samples CS
            INNER JOIN taxonomic_units T ON T.tsn=S.tsn
            INNER JOIN sc_locations L on L.locid=S.collectedlocation
            INNER JOIN sc_samples S ON CS.sampleid=S.sampleid
            INNER JOIN sc_users U on U.userid=S.userid
            INNER JOIN sc_collections C on C.collectionid=CS.collectionid
            LEFT JOIN ( SELECT * FROM
            (SELECT *, ROW_NUMBER() OVER (PARTITION BY csid ORDER BY DATE(date) DESC, csnoteid DESC) AS rownr
            FROM sc_collection_sample_notes ORDER BY csnoteid DESC)
            WHERE rownr = 1) N ON N.csid = CS.csid
            LEFT JOIN (SELECT * FROM vernaculars WHERE
            (language="English" or language="unspecified")) V on V.tsn=T.tsn
            "#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" GROUP BY CS.csid, T.tsn ORDER BY phylo_sort_seq");
        builder
    }

    pub async fn fetch_all(
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_all(pool)
            .await
    }

    pub async fn fetch_one(
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> Result<Self, sqlx::Error> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_one(pool)
            .await
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> Result<Self> {
        let mut builder = Self::build_query(Some(Arc::new(AllocationFilter::Id(id))));
        Ok(builder.build_query_as().fetch_one(pool).await?)
    }

    pub async fn fetch_notes(&mut self, pool: &Pool<Sqlite>) -> Result<()> {
        self.notes = Note::fetch_all(
            Some(Arc::new(note::NoteFilter::CollectionSample(self.id))),
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
            id: row.try_get("csid")?,
            sample: Sample::from_row(row).map(|mut s| {
                s.loaded = true;
                s
            })?,
            collection: Collection::from_row(row).map(|mut c| {
                c.loaded = true;
                c
            })?,
            notes,
            loaded: true,
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
            scripts("users", "locations", "taxa", "assigned-samples")
        )
    ))]
    async fn fetch_allocations(pool: Pool<Sqlite>) {
        async fn check_sample(a: &Allocation, pool: &Pool<Sqlite>) {
            tracing::debug!("loading sample");
            let mut s = Sample::new_loadable(a.sample.id);
            s.load(pool).await.expect("Failed to load sample");
            assert_eq!(a.sample, s);

            let mut c = Collection::new_loadable(a.collection.id);
            c.load(pool).await.expect("Failed to load collection");
            assert_eq!(a.collection, c);
        }

        // check allocations for collection 1
        let assigned =
            Allocation::fetch_all(Some(Arc::new(AllocationFilter::Collection(1))), &pool)
                .await
                .expect("Failed to load assigned samples for first collection");

        assert_eq!(assigned.len(), 2);

        tracing::debug!("{:?}", assigned[0]);
        assert_eq!(assigned[0].sample.id, 1);
        assert_eq!(assigned[0].collection.id, 1);
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
        assert_eq!(assigned[1].collection.id, 1);
        check_sample(&assigned[1], &pool).await;

        // check allocations for collection 2
        let assigned =
            Allocation::fetch_all(Some(Arc::new(AllocationFilter::Collection(2))), &pool)
                .await
                .expect("Failed to load assigned samples for first collection");

        assert_eq!(assigned.len(), 2);

        assert_eq!(assigned[0].sample.id, 1);
        assert_eq!(assigned[0].collection.id, 2);
        check_sample(&assigned[0], &pool).await;

        assert_eq!(assigned[1].sample.id, 3);
        assert_eq!(assigned[1].collection.id, 2);
        check_sample(&assigned[1], &pool).await;

        // check allocations for sample 1
        let assigned = Allocation::fetch_all(Some(Arc::new(AllocationFilter::Sample(1))), &pool)
            .await
            .expect("Failed to load assigned samples for first collection");

        assert_eq!(assigned.len(), 2);

        assert_eq!(assigned[0].sample.id, 1);
        assert_eq!(assigned[0].collection.id, 1);
        check_sample(&assigned[0], &pool).await;

        assert_eq!(assigned[1].sample.id, 1);
        assert_eq!(assigned[1].collection.id, 2);
        check_sample(&assigned[1], &pool).await;
    }
}

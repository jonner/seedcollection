use crate::{
    error::{Error, Result},
    filter::{Cmp, DynFilterPart, FilterBuilder, FilterOp, FilterPart},
    loadable::Loadable,
    note::{self, Note},
    sample::Sample,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteQueryResult, SqliteRow},
    FromRow, Pool, QueryBuilder, Row, Sqlite,
};
use std::sync::Arc;

#[derive(sqlx::FromRow, Debug, Deserialize, Serialize, PartialEq)]
pub struct Collection {
    #[sqlx(rename = "collectionid")]
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allocations: Vec<Allocation>,
    pub userid: i64,
    #[serde(skip_serializing)]
    #[sqlx(skip)]
    loaded: bool,
}

impl Default for Collection {
    fn default() -> Self {
        Self {
            id: -1,
            name: Default::default(),
            description: None,
            allocations: Default::default(),
            userid: -1,
            loaded: false,
        }
    }
}

#[async_trait]
impl Loadable for Collection {
    type Id = i64;

    fn new_loadable(id: Self::Id) -> Self {
        let mut c: Collection = Default::default();
        c.id = id;
        c
    }

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn is_loadable(&self) -> bool {
        self.id > 0
    }

    async fn do_load(&mut self, pool: &Pool<Sqlite>) -> Result<Self> {
        Collection::fetch(self.id, pool).await
    }
}

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

#[derive(Clone)]
pub enum Filter {
    Id(i64),
    User(i64),
    Name(Cmp, String),
    Description(Cmp, String),
}

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(id) => _ = builder.push(" C.collectionid = ").push_bind(*id),
            Self::User(id) => _ = builder.push(" C.userid = ").push_bind(*id),
            Self::Name(cmp, frag) => {
                let s = match cmp {
                    Cmp::Like => format!("%{frag}%"),
                    _ => frag.to_string(),
                };
                builder.push(" C.name ").push(cmp).push_bind(s);
            }
            Self::Description(cmp, frag) => {
                let s = match cmp {
                    Cmp::Like => format!("%{frag}%"),
                    _ => frag.to_string(),
                };
                builder.push(" C.description ").push(cmp).push_bind(s);
            }
        }
    }
}

impl Collection {
    fn build_query(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder = QueryBuilder::new(
            r#"SELECT C.collectionid, C.name, C.description, C.userid, U.username
            FROM sc_collections C INNER JOIN sc_users U ON U.userid=C.userid"#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> Result<Self> {
        Ok(Self::build_query(Some(Arc::new(Filter::Id(id))))
            .build_query_as()
            .fetch_one(pool)
            .await
            .and_then(|mut c: Self| {
                c.loaded = true;
                Ok(c)
            })?)
    }

    pub async fn fetch_all(
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Self>> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_all(pool)
            .await
            .and_then(|mut v| {
                let _ = v.iter_mut().map(|c: &mut Collection| {
                    c.loaded = true;
                    c
                });
                Ok(v)
            })
            .map_err(|e| e.into())
    }

    pub async fn fetch_samples(
        &mut self,
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> Result<()> {
        let mut fbuilder =
            FilterBuilder::new(FilterOp::And).push(Arc::new(AllocationFilter::Collection(self.id)));
        if let Some(filter) = filter {
            fbuilder = fbuilder.push(filter);
        }

        self.allocations = Allocation::fetch_all(Some(fbuilder.build()), pool).await?;
        Ok(())
    }

    pub async fn allocate_sample(
        &mut self,
        sample: Sample,
        pool: &Pool<Sqlite>,
    ) -> Result<SqliteQueryResult> {
        sqlx::query("INSERT INTO sc_collection_samples (collectionid, sampleid) VALUES (?, ?)")
            .bind(self.id)
            .bind(sample.id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
    }

    pub async fn insert(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        sqlx::query("INSERT INTO sc_collections (name, description, userid) VALUES (?, ?, ?)")
            .bind(self.name.clone())
            .bind(self.description.clone())
            .bind(self.userid)
            .execute(pool)
            .await
            .map(|r| {
                self.id = r.last_insert_rowid();
                self.loaded = true;
                r
            })
            .map_err(|e| e.into())
    }

    pub async fn update(&self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.name.is_empty() {
            return Err(Error::InvalidData("No name specified".to_string()));
        }
        if self.id < 0 {
            return Err(Error::InvalidData("No id set".to_string()));
        }
        sqlx::query(
            "UPDATE sc_collections SET name=?, description=?, userid=? WHERE collectionid=?",
        )
        .bind(self.name.clone())
        .bind(self.description.as_ref().cloned())
        .bind(self.userid)
        .bind(self.id)
        .execute(pool)
        .await
        .map_err(|e| e.into())
    }

    pub async fn delete(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(Error::InvalidData(
                "id not set, cannot delete collection".to_string(),
            ));
        }

        sqlx::query("DELETE FROM sc_collections WHERE collectionid=?")
            .bind(self.id)
            .execute(pool)
            .await
            .and_then(|r| {
                self.id = -1;
                Ok(r)
            })
            .map_err(|e| e.into())
    }

    pub fn new(name: String, description: Option<String>, userid: i64) -> Self {
        Self {
            id: -1,
            name,
            description,
            userid,
            allocations: Default::default(),
            loaded: false,
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
            LEFT JOIN ( SELECT * FROM (SELECT *, MAX(date) OVER (PARTITION BY csid) as maxdate from sc_collection_sample_notes)
            WHERE date = maxdate) N ON N.csid = CS.csid
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
            Some(Arc::new(note::FilterField::CollectionSample(self.id))),
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
    use test_log::test;
    use time::Month;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("users"))
    ))]
    async fn test_insert_collections(pool: Pool<Sqlite>) {
        async fn check(pool: &Pool<Sqlite>, name: String, desc: Option<String>, userid: i64) {
            let mut c = Collection::new(name, desc, userid);
            let res = c.insert(&pool).await.expect("failed to insert");
            assert_eq!(res.rows_affected(), 1);
            let mut cload = Collection::new_loadable(res.last_insert_rowid());
            cload.load(&pool).await.expect("Failed to load collection");
            assert_eq!(c, cload);
        }

        check(
            &pool,
            "test name".to_string(),
            Some("Test description".to_string()),
            1,
        )
        .await;

        check(&pool, "test name".to_string(), None, 1).await;
    }

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(
            path = "../../db/fixtures",
            scripts("users", "locations", "taxa", "assigned-samples")
        )
    ))]
    async fn assigned_samples(pool: Pool<Sqlite>) {
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

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
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<AssignedSample>,
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
            samples: Default::default(),
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
pub enum AssignedSampleFilter {
    Id(i64),
    User(i64),
    Collection(i64),
    Sample(i64),
}

impl FilterPart for AssignedSampleFilter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(id) => _ = builder.push(" csid = ").push_bind(*id),
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
            Self::Id(id) => _ = builder.push(" C.id = ").push_bind(*id),
            Self::User(id) => _ = builder.push(" userid = ").push_bind(*id),
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
            r#"SELECT C.id, C.name, C.description, C.userid, U.username
            FROM sc_collections C INNER JOIN sc_users U ON U.id=C.userid"#,
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
        let mut fbuilder = FilterBuilder::new(FilterOp::And)
            .push(Arc::new(AssignedSampleFilter::Collection(self.id)));
        if let Some(filter) = filter {
            fbuilder = fbuilder.push(filter);
        }

        self.samples = AssignedSample::fetch_all(Some(fbuilder.build()), pool).await?;
        Ok(())
    }

    pub async fn assign_sample(
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
        sqlx::query("UPDATE sc_collections SET name=?, description=?, userid=? WHERE id=?")
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

        sqlx::query("DELETE FROM sc_collections WHERE id=?")
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
            samples: Default::default(),
            loaded: false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
pub struct AssignedSample {
    pub id: i64,
    pub sample: Sample,
    pub notes: Vec<Note>,
    pub loaded: bool,
}

impl Default for AssignedSample {
    fn default() -> Self {
        Self {
            id: -1,
            sample: Default::default(),
            notes: Default::default(),
            loaded: false,
        }
    }
}

#[async_trait]
impl Loadable for AssignedSample {
    type Id = i64;

    fn new_loadable(id: Self::Id) -> Self {
        let mut a: AssignedSample = Default::default();
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
        AssignedSample::fetch(self.id, pool)
            .await
            .and_then(|mut a| {
                a.loaded = true;
                Ok(a)
            })
    }
}

impl AssignedSample {
    pub fn build_query(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            r#"
            SELECT CS.id AS csid,

            S.id, quantity, month, year, notes, certainty,

            T.tsn, T.parent_tsn as parentid,
            T.unit_name1, T.unit_name2, T.unit_name3, T.phylo_sort_seq as seq,
            GROUP_CONCAT(V.vernacular_name, "@") as cnames,

            L.locid, L.name as locname, T.complete_name,

            S.userid, U.username

            FROM sc_collection_samples CS
            INNER JOIN taxonomic_units T ON T.tsn=S.tsn
            INNER JOIN sc_locations L on L.locid=S.collectedlocation
            INNER JOIN sc_samples S ON CS.sampleid=S.id
            INNER JOIN sc_users U on U.id=S.userid
            LEFT JOIN (SELECT * FROM vernaculars WHERE
            (language="English" or language="unspecified")) V on V.tsn=T.tsn
            "#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(" GROUP BY S.id, T.tsn ORDER BY phylo_sort_seq");
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
        let mut builder = Self::build_query(Some(Arc::new(AssignedSampleFilter::Id(id))));
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

impl FromRow<'_, SqliteRow> for AssignedSample {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("csid")?,
            sample: Sample::from_row(row)?,
            notes: Default::default(),
            loaded: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_log::test;

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
}

use crate::{
    error::{Error, Result},
    filter::{Cmp, DynFilterPart, FilterBuilder, FilterOp, FilterPart},
    loadable::Loadable,
    sample::Sample,
};
pub use allocation::Allocation;
pub use allocation::AllocationFilter;
use async_trait::async_trait;
pub use note::Note;
pub use note::NoteFilter;
pub use note::NoteType;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Pool, QueryBuilder, Sqlite};
use std::sync::Arc;

pub mod allocation;
pub mod note;

#[derive(sqlx::FromRow, Debug, Deserialize, Serialize, PartialEq)]
pub struct Project {
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

impl Default for Project {
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
impl Loadable for Project {
    type Id = i64;

    fn new_loadable(id: Self::Id) -> Self {
        let mut c: Project = Default::default();
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
        Project::fetch(self.id, pool).await
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

impl Project {
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
                let _ = v.iter_mut().map(|c: &mut Project| {
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
            FilterBuilder::new(FilterOp::And).push(Arc::new(AllocationFilter::Project(self.id)));
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

#[cfg(test)]
mod tests {
    use crate::collection::Project;
    use crate::loadable::Loadable;
    use sqlx::Pool;
    use sqlx::Sqlite;
    use test_log::test;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../../db/fixtures", scripts("users"))
    ))]
    async fn test_insert_collections(pool: Pool<Sqlite>) {
        async fn check(pool: &Pool<Sqlite>, name: String, desc: Option<String>, userid: i64) {
            let mut c = Project::new(name, desc, userid);
            let res = c.insert(&pool).await.expect("failed to insert");
            assert_eq!(res.rows_affected(), 1);
            let mut cload = Project::new_loadable(res.last_insert_rowid());
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

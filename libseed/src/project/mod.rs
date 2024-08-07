//! Objects relating to projects. A Project is a group of seed samples that are intended for a
//! specific purpose. It could be something like a group of seeds that you intend to plant for a
//! particular restoration project, etc.
use crate::{
    error::{Error, Result},
    filter::{Cmp, CompoundFilter, DynFilterPart, FilterPart, Op, SortSpec},
    loadable::{ExternalRef, Loadable},
    sample::Sample,
};
pub use allocation::Allocation;
use async_trait::async_trait;
pub use note::{Note, NoteFilter, NoteType};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Pool, QueryBuilder, Row, Sqlite};
use std::sync::Arc;
use tracing::debug;

pub mod allocation;
pub mod note;

#[derive(sqlx::FromRow, Debug, Deserialize, Serialize, PartialEq)]
pub struct Project {
    #[sqlx(rename = "projectid")]
    pub id: i64,
    #[sqlx(rename = "projname")]
    pub name: String,
    #[sqlx(rename = "projdescription")]
    pub description: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allocations: Vec<Allocation>,
    pub userid: i64,
}

#[async_trait]
impl Loadable for Project {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id
    }

    async fn load(id: Self::Id, pool: &Pool<Sqlite>) -> Result<Self> {
        Self::build_query(Some(Filter::Id(id).into()))
            .build_query_as()
            .fetch_one(pool)
            .await
            .map_err(|e| e.into())
    }

    async fn delete_id(id: &Self::Id, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        sqlx::query("DELETE FROM sc_projects WHERE projectid=?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
    }
}

impl From<Filter> for DynFilterPart {
    fn from(value: Filter) -> Self {
        Arc::new(value)
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
            Self::Id(id) => _ = builder.push(" P.projectid = ").push_bind(*id),
            Self::User(id) => _ = builder.push(" P.userid = ").push_bind(*id),
            Self::Name(cmp, frag) => {
                let s = match cmp {
                    Cmp::Like => format!("%{frag}%"),
                    _ => frag.to_string(),
                };
                builder.push(" P.projname ").push(cmp).push_bind(s);
            }
            Self::Description(cmp, frag) => {
                let s = match cmp {
                    Cmp::Like => format!("%{frag}%"),
                    _ => frag.to_string(),
                };
                builder.push(" P.projdescription ").push(cmp).push_bind(s);
            }
        }
    }
}

impl Project {
    fn build_query(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder = QueryBuilder::new(
            r#"SELECT P.projectid, P.projname, P.projdescription, P.userid, U.username
            FROM sc_projects P INNER JOIN sc_users U ON U.userid=P.userid"#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder
    }

    fn build_count(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut builder = QueryBuilder::new("SELECT COUNT(*) as nprojects FROM sc_projects P");
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder
    }

    pub async fn load_all(filter: Option<DynFilterPart>, pool: &Pool<Sqlite>) -> Result<Vec<Self>> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_all(pool)
            .await
            .map_err(|e| e.into())
    }

    pub async fn count(filter: Option<DynFilterPart>, pool: &Pool<Sqlite>) -> Result<i64> {
        Self::build_count(filter)
            .build()
            .fetch_one(pool)
            .await?
            .try_get("nprojects")
            .map_err(|e| e.into())
    }

    pub async fn load_samples(
        &mut self,
        filter: Option<DynFilterPart>,
        sort: Option<SortSpec<allocation::SortField>>,
        pool: &Pool<Sqlite>,
    ) -> Result<()> {
        let mut fbuilder =
            CompoundFilter::builder(Op::And).push(allocation::Filter::ProjectId(self.id));
        if let Some(filter) = filter {
            fbuilder = fbuilder.push(filter);
        }

        self.allocations = Allocation::load_all(Some(fbuilder.build()), sort, pool).await?;
        Ok(())
    }

    pub async fn allocate_sample(
        &mut self,
        sample: ExternalRef<Sample>,
        pool: &Pool<Sqlite>,
    ) -> Result<SqliteQueryResult> {
        sqlx::query("INSERT INTO sc_project_samples (projectid, sampleid) VALUES (?, ?)")
            .bind(self.id)
            .bind(sample.id())
            .execute(pool)
            .await
            .map_err(|e| e.into())
    }

    pub async fn insert(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        debug!(?self, "Inserting project into database");
        sqlx::query("INSERT INTO sc_projects (projname, projdescription, userid) VALUES (?, ?, ?)")
            .bind(self.name.clone())
            .bind(self.description.clone())
            .bind(self.userid)
            .execute(pool)
            .await
            .map(|r| {
                self.id = r.last_insert_rowid();
                r
            })
            .map_err(|e| e.into())
    }

    pub async fn update(&self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.name.is_empty() {
            return Err(Error::InvalidStateMissingAttribute("name".to_string()));
        }
        if self.id < 0 {
            return Err(Error::InvalidStateMissingAttribute("id".to_string()));
        }
        debug!(?self, "Updating project in database");
        sqlx::query(
            "UPDATE sc_projects SET projname=?, projdescription=?, userid=? WHERE projectid=?",
        )
        .bind(self.name.clone())
        .bind(self.description.as_ref().cloned())
        .bind(self.userid)
        .bind(self.id)
        .execute(pool)
        .await
        .map_err(|e| e.into())
    }

    pub fn new(name: String, description: Option<String>, userid: i64) -> Self {
        Self {
            id: -1,
            name,
            description,
            userid,
            allocations: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::loadable::Loadable;
    use crate::project::Project;
    use sqlx::Pool;
    use sqlx::Sqlite;
    use test_log::test;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../../db/fixtures", scripts("users"))
    ))]
    async fn test_insert_projects(pool: Pool<Sqlite>) {
        async fn check(pool: &Pool<Sqlite>, name: String, desc: Option<String>, userid: i64) {
            let mut c = Project::new(name, desc, userid);
            let res = c.insert(&pool).await.expect("failed to insert");
            assert_eq!(res.rows_affected(), 1);
            let cload = Project::load(res.last_insert_rowid(), &pool)
                .await
                .expect("Failed to load project");
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

//! Seed collection projects
use crate::{
    core::{
        database::Database,
        error::{Error, Result},
        loadable::{ExternalRef, Loadable},
        query::{
            DynFilterPart, LimitSpec, SortSpecs, ToSql,
            filter::{Cmp, FilterPart, and},
        },
    },
    sample::Sample,
};
pub use allocation::AllocatedSample;
use async_trait::async_trait;
pub use note::{Note, NoteFilter, NoteType};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite, sqlite::SqliteRow};
use tracing::debug;

pub mod allocation;
pub mod note;

/// A Project is a group of seed samples that are intended for a specific
/// purpose. It could be something like a group of seeds that you intend to
/// plant for a particular restoration project, etc.
#[derive(sqlx::FromRow, Debug, Deserialize, Serialize, PartialEq)]
pub struct Project {
    /// A unique ID that identifies this project in the database
    #[sqlx(rename = "projectid")]
    pub id: i64,

    /// A short name for this project
    #[sqlx(rename = "projname")]
    pub name: String,

    /// A textual description of this project
    #[sqlx(rename = "projdescription")]
    pub description: Option<String>,

    /// The collection of samples associated with this project
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allocations: Vec<AllocatedSample>,

    /// The user that owns this project
    pub userid: i64,
}

#[async_trait]
impl Loadable for Project {
    type Id = i64;
    type Sort = SortField;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id
    }

    async fn insert(&mut self, db: &Database) -> Result<Self::Id> {
        debug!(?self, "Inserting project into database");
        let newval = sqlx::query_as(
            "INSERT INTO sc_projects
                (projname, projdescription, userid)
            VALUES
                (?, ?, ?)
            RETURNING *",
        )
        .bind(self.name.clone())
        .bind(self.description.clone())
        .bind(self.userid)
        .fetch_one(db.pool())
        .await?;
        *self = newval;
        Ok(self.id)
    }

    async fn load(id: Self::Id, db: &Database) -> Result<Self> {
        Self::query_builder(Some(Filter::Id(id).into()), None, None)
            .build_query_as()
            .fetch_one(db.pool())
            .await
            .map_err(|e| e.into())
    }

    async fn load_all(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<Self::Sort>>,
        limit: Option<LimitSpec>,
        db: &Database,
    ) -> Result<Vec<Self>> {
        Self::query_builder(filter, sort, limit)
            .build_query_as()
            .fetch_all(db.pool())
            .await
            .map_err(|e| e.into())
    }

    async fn delete_id(id: &Self::Id, db: &Database) -> Result<()> {
        sqlx::query("DELETE FROM sc_projects WHERE projectid=?")
            .bind(id)
            .execute(db.pool())
            .await
            .map_err(|e| e.into())
            .map(|_| ())
    }

    async fn update(&self, db: &Database) -> Result<()> {
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
        .execute(db.pool())
        .await?;
        Ok(())
    }
}

/// A type for specifying fields for filtering a [Project]
#[derive(Clone)]
pub enum Filter {
    /// Filter by project ID
    Id(i64),

    /// Filter by the id of the user that owns the project
    User(i64),

    /// Filter by a string that uses [Cmp] to compare to the project name
    Name(Cmp, String),

    /// Filter by a string that uses [Cmp] to compare to the project description
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

pub enum SortField {
    Id,
    Name,
    UserId,
}

impl ToSql for SortField {
    fn to_sql(&self) -> String {
        match self {
            SortField::Id => "P.projectid".into(),
            SortField::Name => "P.projname".into(),
            SortField::UserId => "P.userid".into(),
        }
    }
}

impl Project {
    fn query_builder(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<<Self as Loadable>::Sort>>,
        limit: Option<LimitSpec>,
    ) -> QueryBuilder<'static, Sqlite> {
        let mut builder = QueryBuilder::new(
            r#"SELECT P.projectid, P.projname, P.projdescription, P.userid, U.username
            FROM sc_projects P INNER JOIN sc_users U ON U.userid=P.userid"#,
        );
        if let Some(f) = filter {
            builder.push(" WHERE ");
            f.add_to_query(&mut builder);
        }
        builder.push(sort.unwrap_or(SortField::Id.into()).to_sql());
        if let Some(l) = limit {
            builder.push(l.to_sql());
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

    /// query the number of matching projects in the database
    pub async fn count(filter: Option<DynFilterPart>, db: &Database) -> Result<i64> {
        Self::build_count(filter)
            .build()
            .fetch_one(db.pool())
            .await?
            .try_get("nprojects")
            .map_err(|e| e.into())
    }

    /// Load all of the samples that are allocated to this project
    pub async fn load_samples(
        &mut self,
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<allocation::SortField>>,
        db: &Database,
    ) -> Result<()> {
        let mut fbuilder = and().push(allocation::Filter::ProjectId(self.id));
        if let Some(filter) = filter {
            fbuilder = fbuilder.push(filter);
        }

        self.allocations =
            AllocatedSample::load_all(Some(fbuilder.build()), sort, None, db).await?;
        Ok(())
    }

    /// Allocate the given sample to this project
    pub async fn allocate_sample(
        &mut self,
        sample: Sample,
        db: &Database,
    ) -> Result<<AllocatedSample as Loadable>::Id> {
        let mut allocation = AllocatedSample {
            id: AllocatedSample::invalid_id(),
            sample,
            projectid: self.id,
            notes: Default::default(),
        };
        allocation.insert(db).await
    }

    /// Create a new project with the given data. It will initially have an
    /// invalid ID until it is inserted into the database.
    pub fn new(name: String, description: Option<String>, userid: i64) -> Self {
        Self {
            id: Self::invalid_id(),
            name,
            description,
            userid,
            allocations: Default::default(),
        }
    }
}

impl FromRow<'_, SqliteRow> for ExternalRef<Project> {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Project::from_row(row)
            .map(ExternalRef::Object)
            .or_else(|_| row.try_get("projectid").map(ExternalRef::Stub))
    }
}

#[cfg(test)]
mod tests {
    use crate::core::database::Database;
    use crate::core::loadable::Loadable;
    use crate::project::Project;
    use sqlx::Pool;
    use sqlx::Sqlite;
    use test_log::test;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../../db/fixtures", scripts("users"))
    ))]
    async fn test_insert_projects(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        async fn check(db: &Database, name: String, desc: Option<String>, userid: i64) {
            let mut c = Project::new(name, desc, userid);
            let id = c.insert(db).await.expect("failed to insert");
            let cload = Project::load(id, db).await.expect("Failed to load project");
            assert_eq!(c, cload);
        }

        check(
            &db,
            "test name".to_string(),
            Some("Test description".to_string()),
            1,
        )
        .await;

        check(&db, "test name".to_string(), None, 1).await;
    }
}

use crate::{
    core::{
        error::Result,
        loadable::{ExternalRef, Loadable},
        query::{DynFilterPart, LimitSpec, SortSpecs, ToSql, filter::FilterPart},
    },
    user::User,
};
use sqlx::{FromRow, QueryBuilder, Row, Sqlite, sqlite::SqliteRow};
use std::num::NonZero;

const DEFAULT_PAGESIZE: NonZero<u32> = NonZero::new(50).unwrap();

#[derive(sqlx::FromRow, Clone, Debug, PartialEq)]
pub struct Preferences {
    #[sqlx(rename = "prefid")]
    id: <Self as Loadable>::Id,
    userid: <User as Loadable>::Id,
    pub pagesize: NonZero<u32>,
}

pub enum SortField {
    Id,
    Userid,
}

impl ToSql for SortField {
    fn to_sql(&self) -> String {
        match self {
            SortField::Id => "prefid".into(),
            SortField::Userid => "userid".into(),
        }
    }
}

pub enum Filter {
    Id(<Preferences as Loadable>::Id),
    Userid(<User as Loadable>::Id),
}

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Filter::Id(id) => builder.push("prefid IS ").push_bind(*id),
            Filter::Userid(id) => builder.push("userid IS ").push_bind(*id),
        };
    }
}

impl Loadable for Preferences {
    type Id = i64;

    type Sort = SortField;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_invalid(&mut self) {
        self.id = Self::invalid_id()
    }

    async fn insert(&mut self, db: &crate::Database) -> Result<&Self::Id> {
        let newval = sqlx::query_as(
            "INSERT INTO sc_user_prefs (userid, pagesize) VALUES (?, ?) RETURNING *",
        )
        .bind(self.userid)
        .bind(self.pagesize)
        .fetch_one(db.pool())
        .await?;
        *self = newval;
        Ok(&self.id)
    }

    async fn load(id: Self::Id, db: &crate::Database) -> Result<Self>
    where
        Self: Sized,
    {
        Self::query_builder(Some(Filter::Id(id).into()), None, None)
            .build_query_as()
            .fetch_one(db.pool())
            .await
            .map_err(Into::into)
    }

    async fn load_all(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<Self::Sort>>,
        limit: Option<LimitSpec>,
        db: &crate::Database,
    ) -> Result<Vec<Self>>
    where
        Self: Sized,
    {
        Self::query_builder(filter, sort, limit)
            .build_query_as()
            .fetch_all(db.pool())
            .await
            .map_err(Into::into)
    }

    async fn count(
        filter: Option<crate::core::query::DynFilterPart>,
        db: &crate::Database,
    ) -> Result<u64> {
        Self::count_query_builder(filter)
            .build()
            .fetch_one(db.pool())
            .await?
            .try_get("count")
            .map_err(Into::into)
    }

    async fn delete_id(id: &Self::Id, db: &crate::Database) -> Result<()> {
        sqlx::query("DELETE FROM sc_user_prefs WHERE prefid IS ?")
            .bind(id)
            .execute(db.pool())
            .await
            .map(|_| ())
            .map_err(Into::into)
    }

    async fn update(&self, db: &crate::Database) -> Result<()> {
        sqlx::query(
            "UPDATE sc_user_prefs
                SET
                    userid=?,
                    pagesize=?
                WHERE prefid=?",
        )
        .bind(self.userid)
        .bind(self.pagesize)
        .bind(self.id)
        .execute(db.pool())
        .await
        .map(|_| ())
        .map_err(Into::into)
    }
}

impl Preferences {
    pub fn new(userid: <User as Loadable>::Id, pagesize: Option<NonZero<u32>>) -> Self {
        Self {
            id: Self::invalid_id(),
            userid,
            pagesize: pagesize.unwrap_or(DEFAULT_PAGESIZE),
        }
    }

    fn base_query_builder<'q>(
        select_fields: &Vec<&str>,
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<<Self as Loadable>::Sort>>,
        limit: Option<LimitSpec>,
    ) -> QueryBuilder<'q, Sqlite> {
        let fields = select_fields.join(", ");
        let mut builder = QueryBuilder::new("SELECT ");
        builder.push(fields);
        builder.push(" FROM sc_user_prefs ");
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

    fn query_builder<'q>(
        filter: Option<DynFilterPart>,
        sort: Option<SortSpecs<<Self as Loadable>::Sort>>,
        limit: Option<LimitSpec>,
    ) -> QueryBuilder<'q, Sqlite> {
        Self::base_query_builder(&vec!["*"], filter, sort, limit)
    }

    fn count_query_builder<'q>(filter: Option<DynFilterPart>) -> QueryBuilder<'q, Sqlite> {
        Self::base_query_builder(&vec!["COUNT(*) as count"], filter, None, None)
    }
}

impl FromRow<'_, SqliteRow> for ExternalRef<Preferences> {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        tracing::debug!("initializing Preferences from db row");
        Preferences::from_row(row)
            .map(ExternalRef::Object)
            .or_else(|_| row.try_get("prefid").map(ExternalRef::Stub))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Database;
    use sqlx::Pool;
    use test_log::test;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../../db/fixtures", scripts("users"))
    ))]
    async fn register_user(pool: Pool<Sqlite>) {
        let db = Database::from(pool);
        let mut user = User::load(1, &db).await.expect("Failed to load user");
        assert_eq!(user.prefs, None);
        let prefs = user
            .preferences_mut(&db)
            .await
            .expect("Failed to fetch preferences");
        assert_eq!(prefs.userid, 1);
        assert_ne!(prefs.id, Preferences::invalid_id());
        prefs.pagesize = 25.try_into().unwrap();
        prefs
            .update(&db)
            .await
            .expect("Failed to update preferences");

        // re-load from db and ensure it's updated
        let newprefs = Preferences::load(prefs.id, &db)
            .await
            .expect("Failed to re-load preferences");
        assert_eq!(newprefs.pagesize.get(), 25);
    }
}

//! Objects to manage the origin of seed samples
use crate::{
    Database,
    error::{Error, Result},
    loadable::{ExternalRef, Loadable},
    query::{Cmp, CompoundFilter, DynFilterPart, FilterPart, Op},
};
use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use sqlx::QueryBuilder;
use sqlx::Sqlite;
use sqlx::{
    prelude::*,
    sqlite::{SqliteQueryResult, SqliteRow},
};
use std::sync::Arc;

/// A type for specifying fields that can be used for filtering a database query
/// for sources
#[derive(Clone)]
pub enum Filter {
    /// Match the ID of the source to the given value
    Id(i64),

    /// Match the id of the source's user to the given value
    UserId(i64),

    /// Compare the name of the source to the given value
    Name(Cmp, String),

    /// Compare the description of the source to the given value
    Description(Cmp, String),
}

impl From<Filter> for DynFilterPart {
    fn from(value: Filter) -> Self {
        Arc::new(value)
    }
}

impl From<Filter> for Option<DynFilterPart> {
    fn from(value: Filter) -> Self {
        Some(Arc::new(value))
    }
}

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(id) => _ = builder.push(" L.srcid = ").push_bind(*id),
            Self::UserId(id) => _ = builder.push(" L.userid = ").push_bind(*id),
            Self::Name(cmp, frag) => {
                let s = match cmp {
                    Cmp::Like => format!("%{frag}%"),
                    _ => frag.to_string(),
                };
                builder.push(" L.srcname ").push(cmp).push_bind(s);
            }
            Filter::Description(cmp, frag) => {
                let s = match cmp {
                    Cmp::Like => format!("%{frag}%"),
                    _ => frag.to_string(),
                };
                builder.push(" L.srcdesc ").push(cmp).push_bind(s);
            }
        }
    }
}

/// A data type that represents a source from which a seed sample was acquired.
/// This could be a vendor or a location where seed was collected.
#[derive(Debug, sqlx::FromRow, Deserialize, Serialize, PartialEq, Clone)]
pub struct Source {
    /// A unique ID that identifies this source in the database
    #[sqlx(rename = "srcid")]
    pub id: i64,

    /// The name of the source
    #[sqlx(rename = "srcname")]
    pub name: String,

    /// An optional longer description for this source
    #[sqlx(rename = "srcdesc", default)]
    pub description: Option<String>,

    /// An optional latitude specifying the location of the source
    #[sqlx(default)]
    pub latitude: Option<f64>,

    /// An optional longitude specifying the location of the source
    #[sqlx(default)]
    pub longitude: Option<f64>,

    /// The database user to whom this source belongs
    pub userid: i64,
}

#[async_trait]
impl Loadable for Source {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn set_id(&mut self, id: Self::Id) {
        self.id = id
    }

    async fn load(id: Self::Id, db: &Database) -> Result<Self> {
        Self::build_query(Some(Filter::Id(id).into()))
            .build_query_as()
            .fetch_one(db.pool())
            .await
            .map_err(|e| e.into())
    }

    async fn delete_id(id: &Self::Id, db: &Database) -> Result<SqliteQueryResult> {
        sqlx::query(r#"DELETE FROM sc_sources WHERE srcid=?1"#)
            .bind(id)
            .execute(db.pool())
            .await
            .map_err(|e| e.into())
    }
}

impl Source {
    fn build_query(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut qb = QueryBuilder::new(
            r#"SELECT L.srcid, L.srcname, L.srcdesc, L.latitude, L.longitude,
            L.userid, U.username FROM sc_sources L
            INNER JOIN sc_users U ON U.userid=L.userid"#,
        );
        if let Some(f) = filter {
            qb.push(" WHERE ");
            f.add_to_query(&mut qb);
        }
        qb.push(" ORDER BY srcname ASC");
        qb
    }

    fn build_count(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut qb = QueryBuilder::new("SELECT COUNT(*) as nsources FROM sc_sources L");
        if let Some(f) = filter {
            qb.push(" WHERE ");
            f.add_to_query(&mut qb);
        }
        qb
    }

    /// Loads all matching sources from the database
    pub async fn load_all(filter: Option<DynFilterPart>, db: &Database) -> Result<Vec<Source>> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_all(db.pool())
            .await
            .map_err(|e| e.into())
    }

    /// Loads all matching sources from the database for the given user
    pub async fn load_all_user(
        userid: i64,
        filter: Option<DynFilterPart>,
        db: &Database,
    ) -> Result<Vec<Source>> {
        let mut fbuilder = CompoundFilter::builder(Op::And).push(Filter::UserId(userid));
        if let Some(f) = filter {
            fbuilder = fbuilder.push(f);
        }
        Self::load_all(Some(fbuilder.build()), db).await
    }

    pub async fn count(filter: Option<DynFilterPart>, db: &Database) -> Result<i64> {
        Self::build_count(filter)
            .build()
            .fetch_one(db.pool())
            .await?
            .try_get("nsources")
            .map_err(|e| e.into())
    }

    /// Add this source to the database. If this call completes successfully,
    /// the id of this object will be updated to the ID of the inserted row in the
    /// database
    pub async fn insert(&mut self, db: &Database) -> Result<SqliteQueryResult> {
        if self.id != -1 {
            return Err(Error::InvalidInsertObjectAlreadyExists(self.id));
        }

        sqlx::query(
            r#"INSERT INTO sc_sources
          (srcname, srcdesc, latitude, longitude, userid)
          VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(&self.name)
        .bind(&self.description)
        .bind(self.latitude)
        .bind(self.longitude)
        .bind(self.userid)
        .execute(db.pool())
        .await
        .inspect(|r| self.id = r.last_insert_rowid())
        .map_err(|e| e.into())
    }

    /// Update the source in the database such that it matches this object
    pub async fn update(&self, db: &Database) -> Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(Error::InvalidUpdateObjectNotFound);
        }

        sqlx::query(
            "UPDATE sc_sources SET srcname=?, srcdesc=?, latitude=?, longitude=? WHERE srcid=?",
        )
        .bind(self.name.clone())
        .bind(self.description.as_ref().cloned())
        .bind(self.latitude)
        .bind(self.longitude)
        .bind(self.id)
        .execute(db.pool())
        .await
        .map_err(|e| e.into())
    }

    /// Creates a new source object with the given data. It will initially have
    /// an invalid ID until it is inserted into the database
    pub fn new(
        name: String,
        description: Option<String>,
        latitude: Option<f64>,
        longitude: Option<f64>,
        userid: i64,
    ) -> Self {
        Self {
            id: Self::invalid_id(),
            name,
            description,
            latitude,
            longitude,
            userid,
        }
    }
}

impl FromRow<'_, SqliteRow> for ExternalRef<Source> {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Source::from_row(row)
            .map(ExternalRef::Object)
            .or_else(|_| row.try_get("tsn").map(ExternalRef::Stub))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Pool;
    use test_log::test;

    #[test(sqlx::test(
        migrations = "../db/migrations/",
        fixtures(path = "../../db/fixtures", scripts("users"))
    ))]
    async fn test_insert_sources(pool: Pool<Sqlite>) {
        let db = Database(pool);
        async fn check(
            db: &Database,
            name: String,
            desc: Option<String>,
            lat: Option<f64>,
            lon: Option<f64>,
            userid: i64,
        ) {
            let mut src = Source::new(name, desc, lat, lon, userid);
            // full data
            let res = src.insert(db).await.expect("failed to insert");
            assert_eq!(res.rows_affected(), 1);
            let srcloaded = Source::load(res.last_insert_rowid(), db)
                .await
                .expect("Failed to load inserted object");
            assert_eq!(src, srcloaded);
        }

        check(
            &db,
            "test name".to_string(),
            Some("Test description".to_string()),
            Some(39.7870909115992),
            Some(-75.64827694159666),
            1,
        )
        .await;
        check(
            &db,
            "test name".to_string(),
            Some("Test description".to_string()),
            Some(39.7870909115992),
            None,
            1,
        )
        .await;
        check(
            &db,
            "test name".to_string(),
            Some("Test description".to_string()),
            None,
            None,
            1,
        )
        .await;
        check(&db, "test name".to_string(), None, None, None, 1).await;
        check(&db, "".to_string(), None, None, None, 1).await;
    }
}

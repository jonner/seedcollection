//! Objects to keep track of the origin of seed samples
use crate::{
    error::{Error, Result},
    filter::{Cmp, DynFilterPart, FilterPart},
    loadable::{ExternalRef, Loadable},
};
use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use sqlx::Pool;
use sqlx::QueryBuilder;
use sqlx::Sqlite;
use sqlx::{
    prelude::*,
    sqlite::{SqliteQueryResult, SqliteRow},
};
use std::sync::Arc;

#[derive(Debug, sqlx::FromRow, Deserialize, Serialize, PartialEq)]
pub struct Source {
    #[sqlx(rename = "srcid")]
    pub id: i64,
    #[sqlx(rename = "srcname")]
    pub name: String,
    #[sqlx(rename = "srcdesc", default)]
    pub description: Option<String>,
    #[sqlx(default)]
    pub latitude: Option<f64>,
    #[sqlx(default)]
    pub longitude: Option<f64>,
    pub userid: i64,
}

impl FromRow<'_, SqliteRow> for ExternalRef<Source> {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Source::from_row(row)
            .map(|t| ExternalRef::Object(t))
            .or_else(|_| row.try_get("tsn").map(|id| ExternalRef::Stub(id)))
    }
}

#[async_trait]
impl Loadable for Source {
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
        Source::fetch(id, pool).await
    }

    async fn delete_id(id: &Self::Id, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        sqlx::query(r#"DELETE FROM sc_sources WHERE srcid=?1"#)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
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
            Self::Id(id) => _ = builder.push(" L.srcid = ").push_bind(*id),
            Self::User(id) => _ = builder.push(" L.userid = ").push_bind(*id),
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

const MAP_TILER_KEY: &str = "OfKZsQq0kXBWp83M3Wjx";

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
        let mut qb = QueryBuilder::new(
            r#"SELECT
                COUNT(*) as nsources
            FROM (
                SELECT
                    L.srcid,
                    L.srcname,
                    L.srcdesc,
                    L.latitude,
                    L.longitude,
                    L.userid,
                    U.username
                FROM
                    sc_sources L
                INNER JOIN sc_users U ON U.userid=L.userid"#,
        );
        if let Some(f) = filter {
            qb.push(" WHERE ");
            f.add_to_query(&mut qb);
        }
        qb.push(")");
        qb
    }

    pub fn map_viewer_uri(&self, zoom: f32) -> Option<String> {
        match (self.latitude, self.longitude) {
            (Some(latitude), Some(longitude)) => Some(format!("https://api.maptiler.com/maps/topo-v2/?key={MAP_TILER_KEY}#{zoom}/{latitude}/{longitude}")),
            _ => None,
        }
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> Result<Source> {
        Self::build_query(Some(Arc::new(Filter::Id(id))))
            .build_query_as()
            .fetch_one(pool)
            .await
            .map_err(|e| e.into())
    }

    pub async fn fetch_all(
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Source>> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_all(pool)
            .await
            .map_err(|e| e.into())
    }

    pub async fn fetch_all_user(userid: i64, pool: &Pool<Sqlite>) -> Result<Vec<Source>> {
        Self::fetch_all(Some(Arc::new(Filter::User(userid))), pool).await
    }

    pub async fn count(filter: Option<DynFilterPart>, pool: &Pool<Sqlite>) -> Result<i64> {
        Self::build_count(filter)
            .build()
            .fetch_one(pool)
            .await?
            .try_get("nsources")
            .map_err(|e| e.into())
    }

    pub async fn insert(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.id != -1 {
            return Err(Error::InvalidData(
                "Location is is not -1, cannot insert a new item".to_string(),
            ));
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
        .execute(pool)
        .await
        .map(|r| {
            self.id = r.last_insert_rowid();
            r
        })
        .map_err(|e| e.into())
    }

    pub async fn update(&self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(Error::InvalidData(
                "Id is not set, cannot update".to_string(),
            ));
        }

        sqlx::query(
            "UPDATE sc_sources SET srcname=?, srcdesc=?, latitude=?, longitude=? WHERE srcid=?",
        )
        .bind(self.name.clone())
        .bind(self.description.as_ref().cloned())
        .bind(self.latitude)
        .bind(self.longitude)
        .bind(self.id)
        .execute(pool)
        .await
        .map_err(|e| e.into())
    }

    pub async fn delete(&mut self, pool: &Pool<Sqlite>) -> Result<SqliteQueryResult> {
        sqlx::query(r#"DELETE FROM sc_sources WHERE srcid=?1"#)
            .bind(self.id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
            .and_then(|r| {
                self.id = -1;
                Ok(r)
            })
    }

    pub fn new(
        name: String,
        description: Option<String>,
        latitude: Option<f64>,
        longitude: Option<f64>,
        userid: i64,
    ) -> Self {
        Self {
            id: -1,
            name,
            description,
            latitude,
            longitude,
            userid,
        }
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
    async fn test_insert_sources(pool: Pool<Sqlite>) {
        async fn check(
            pool: &Pool<Sqlite>,
            name: String,
            desc: Option<String>,
            lat: Option<f64>,
            lon: Option<f64>,
            userid: i64,
        ) {
            let mut src = Source::new(name, desc, lat, lon, userid);
            // full data
            let res = src.insert(&pool).await.expect("failed to insert");
            assert_eq!(res.rows_affected(), 1);
            let srcloaded = Source::load(res.last_insert_rowid(), &pool)
                .await
                .expect("Failed to load inserted object");
            assert_eq!(src, srcloaded);
        }

        check(
            &pool,
            "test name".to_string(),
            Some("Test description".to_string()),
            Some(39.7870909115992),
            Some(-75.64827694159666),
            1,
        )
        .await;
        check(
            &pool,
            "test name".to_string(),
            Some("Test description".to_string()),
            Some(39.7870909115992),
            None,
            1,
        )
        .await;
        check(
            &pool,
            "test name".to_string(),
            Some("Test description".to_string()),
            None,
            None,
            1,
        )
        .await;
        check(&pool, "test name".to_string(), None, None, None, 1).await;
        check(&pool, "".to_string(), None, None, None, 1).await;
    }
}

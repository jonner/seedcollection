use crate::{
    error::{Error, Result},
    filter::{Cmp, DynFilterPart, FilterPart},
    loadable::Loadable,
};
use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use sqlx::sqlite::SqliteQueryResult;
use sqlx::Pool;
use sqlx::QueryBuilder;
use sqlx::Sqlite;
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

impl Default for Source {
    fn default() -> Self {
        Self {
            id: -1,
            name: Default::default(),
            description: None,
            latitude: None,
            longitude: None,
            userid: -1,
        }
    }
}

#[async_trait]
impl Loadable for Source {
    type Id = i64;

    fn new_loadable(id: Self::Id) -> Self {
        let mut src: Self = Default::default();
        src.id = id;
        src
    }

    fn is_loadable(&self) -> bool {
        self.id > 0
    }

    async fn do_load(&mut self, pool: &Pool<Sqlite>) -> Result<Self> {
        Source::fetch(self.id, pool).await
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
                builder.push(" locdescription ").push(cmp).push_bind(s);
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
            let mut srcloaded = Source::new_loadable(res.last_insert_rowid());
            let res = srcloaded.load(&pool).await;

            if let Err(e) = res {
                println!("{e:?}");
                panic!();
            }
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

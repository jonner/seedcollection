use crate::filter::Cmp;
use crate::filter::DynFilterPart;
use anyhow::anyhow;
use serde::Deserialize;
use serde::Serialize;
use sqlx::sqlite::SqliteQueryResult;
use sqlx::Pool;
use sqlx::QueryBuilder;
use sqlx::Sqlite;
use std::sync::Arc;

use crate::filter::FilterPart;

#[derive(Debug, sqlx::FromRow, Deserialize, Serialize)]
pub struct Location {
    #[sqlx(rename = "locid")]
    pub id: i64,
    #[sqlx(rename = "locname")]
    pub name: String,
    #[sqlx(default)]
    pub description: Option<String>,
    #[sqlx(default)]
    pub latitude: Option<f64>,
    #[sqlx(default)]
    pub longitude: Option<f64>,
    pub userid: Option<i64>,
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
            Self::Id(id) => _ = builder.push(" L.locid = ").push_bind(*id),
            Self::User(id) => _ = builder.push(" L.userid = ").push_bind(*id),
            Self::Name(cmp, frag) => {
                let s = match cmp {
                    Cmp::Like => format!("%{frag}%"),
                    _ => frag.to_string(),
                };
                builder.push(" L.name ").push(cmp).push_bind(s);
            }
            Filter::Description(cmp, frag) => {
                let s = match cmp {
                    Cmp::Like => format!("%{frag}%"),
                    _ => frag.to_string(),
                };
                builder.push(" L.description ").push(cmp).push_bind(s);
            }
        }
    }
}

const MAP_TILER_KEY: &str = "OfKZsQq0kXBWp83M3Wjx";

impl Location {
    fn build_query(filter: Option<DynFilterPart>) -> QueryBuilder<'static, Sqlite> {
        let mut qb = QueryBuilder::new(
            r#"SELECT L.locid, L.name as locname, L.description, L.latitude, L.longitude,
            L.userid, U.username FROM sc_locations L
            INNER JOIN sc_users U ON U.id=L.userid"#,
        );
        if let Some(f) = filter {
            qb.push(" WHERE ");
            f.add_to_query(&mut qb);
        }
        qb.push(" ORDER BY name ASC");
        qb
    }

    pub fn map_viewer_uri(&self, zoom: f32) -> Option<String> {
        match (self.latitude, self.longitude) {
            (Some(latitude), Some(longitude)) => Some(format!("https://api.maptiler.com/maps/topo-v2/?key={MAP_TILER_KEY}#{zoom}/{latitude}/{longitude}")),
            _ => None,
        }
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> anyhow::Result<Location> {
        Ok(Self::build_query(Some(Arc::new(Filter::Id(id))))
            .build_query_as()
            .fetch_one(pool)
            .await?)
    }

    pub async fn fetch_all(
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> anyhow::Result<Vec<Location>> {
        Ok(Self::build_query(filter)
            .build_query_as()
            .fetch_all(pool)
            .await?)
    }

    pub async fn fetch_all_user(userid: i64, pool: &Pool<Sqlite>) -> anyhow::Result<Vec<Location>> {
        Ok(Self::build_query(Some(Arc::new(Filter::User(userid))))
            .build_query_as()
            .fetch_all(pool)
            .await?)
    }

    pub async fn insert(&self, pool: &Pool<Sqlite>) -> anyhow::Result<SqliteQueryResult> {
        if self.id != -1 {
            return Err(anyhow!("Location is is not -1, cannot insert a new item"));
        }

        sqlx::query(
            r#"INSERT INTO sc_locations
          (name, description, latitude, longitude, userid)
          VALUES (?, ?, ?, ?, ?)"#,
        )
        .bind(&self.name)
        .bind(&self.description)
        .bind(self.latitude)
        .bind(self.longitude)
        .bind(self.userid)
        .execute(pool)
        .await
        .map_err(|e| e.into())
    }

    pub fn new(
        name: String,
        description: Option<String>,
        latitude: Option<f64>,
        longitude: Option<f64>,
        userid: Option<i64>,
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

    // this just creates a placeholder object to hold an ID so that another object (e.g. sample)
    // that contains a Taxon object can still exist without loading the entire taxon from the
    // database
    pub fn new_id_only(id: i64) -> Self {
        Self {
            id,
            name: Default::default(),
            description: None,
            latitude: None,
            longitude: None,
            userid: None,
        }
    }
}

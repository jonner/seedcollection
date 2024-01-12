use crate::{
    filter::{Cmp, DynFilterPart, FilterBuilder, FilterOp, FilterPart},
    sample::Sample,
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, FromRow, Pool, QueryBuilder, Row, Sqlite};
use std::sync::Arc;

#[derive(sqlx::FromRow, Deserialize, Serialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<AssignedSample>,
    pub userid: i64,
}

#[derive(Clone)]
pub enum AssignedSampleFilter {
    Id(i64),
    Collection(i64),
    Sample(i64),
}

impl FilterPart for AssignedSampleFilter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Id(id) => _ = builder.push(" csid = ").push_bind(*id),
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

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> anyhow::Result<Self> {
        Ok(Self::build_query(Some(Arc::new(Filter::Id(id))))
            .build_query_as()
            .fetch_one(pool)
            .await?)
    }

    pub async fn fetch_all(
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> anyhow::Result<Vec<Self>> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_all(pool)
            .await
            .map_err(|e| e.into())
    }

    pub async fn fetch_samples(
        &mut self,
        filter: Option<DynFilterPart>,
        pool: &Pool<Sqlite>,
    ) -> anyhow::Result<()> {
        let mut fbuilder = FilterBuilder::new(FilterOp::And)
            .push(Arc::new(AssignedSampleFilter::Collection(self.id)));
        if let Some(filter) = filter {
            fbuilder = fbuilder.push(filter);
        }

        self.samples = AssignedSample::fetch_all(Some(fbuilder.build()), pool).await?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
pub struct AssignedSample {
    pub id: i64,
    pub sample: Sample,
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

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> anyhow::Result<Self> {
        let mut builder = Self::build_query(Some(Arc::new(AssignedSampleFilter::Id(id))));
        Ok(builder.build_query_as().fetch_one(pool).await?)
    }
}

impl FromRow<'_, SqliteRow> for AssignedSample {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("csid")?,
            sample: Sample::from_row(row)?,
        })
    }
}

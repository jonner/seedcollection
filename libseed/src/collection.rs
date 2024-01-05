use crate::{filter::FilterPart, sample::Sample};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, FromRow, Pool, QueryBuilder, Row, Sqlite};

#[derive(sqlx::FromRow, Deserialize, Serialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<AssignedSample>,
}

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

impl Collection {
    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> anyhow::Result<Self> {
        Ok(
            sqlx::query_as("SELECT id, name, description FROM sc_collections WHERE id=?")
                .bind(id)
                .fetch_one(pool)
                .await?,
        )
    }

    pub async fn fetch_all(pool: &Pool<Sqlite>) -> anyhow::Result<Vec<Self>> {
        Ok(
            sqlx::query_as("SELECT id, name, description FROM sc_collections")
                .fetch_all(pool)
                .await?,
        )
    }

    pub async fn fetch_samples(&mut self, pool: &Pool<Sqlite>) -> anyhow::Result<()> {
        self.samples = AssignedSample::fetch_all(
            Some(Box::new(AssignedSampleFilter::Collection(self.id))),
            pool,
        )
        .await?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
pub struct AssignedSample {
    pub id: i64,
    pub sample: Sample,
}

impl AssignedSample {
    pub fn build_query(filter: Option<Box<dyn FilterPart>>) -> QueryBuilder<'static, Sqlite> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            r#"
            SELECT CS.id AS csid,

            S.id, quantity, month, year, notes, certainty,

            T.tsn, T.parent_tsn as parentid,
            T.unit_name1, T.unit_name2, T.unit_name3, T.phylo_sort_seq as seq,
            GROUP_CONCAT(V.vernacular_name, "@") as cnames,

            L.locid, L.name as locname, T.complete_name

            FROM sc_collection_samples CS
            INNER JOIN taxonomic_units T ON T.tsn=S.tsn
            INNER JOIN sc_locations L on L.locid=S.collectedlocation
            INNER JOIN sc_samples S ON CS.sampleid=S.id
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
        filter: Option<Box<dyn FilterPart>>,
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        Self::build_query(filter)
            .build_query_as()
            .fetch_all(pool)
            .await
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> anyhow::Result<Self> {
        let mut builder = Self::build_query(Some(Box::new(AssignedSampleFilter::Id(id))));
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

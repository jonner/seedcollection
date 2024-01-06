use crate::{
    filter::{Cmp, CompoundFilter, FilterOp, FilterPart},
    location::Location,
    taxonomy::Taxon,
    user::User,
};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteRow, FromRow, Pool, QueryBuilder, Row, Sqlite};

#[derive(Deserialize, Serialize, Debug, sqlx::Type)]
#[repr(i32)]
pub enum Certainty {
    Certain = 1,
    Uncertain = 2,
}

#[derive(Deserialize, Serialize)]
pub struct Sample {
    pub id: i64,
    pub user: User,
    pub taxon: Taxon,
    pub location: Location,
    pub quantity: Option<i64>,
    pub month: Option<u32>,
    pub year: Option<u32>,
    pub notes: Option<String>,
    pub certainty: Certainty,
}

pub enum Filter {
    Sample(Cmp, i64),
    SampleNotIn(Vec<i64>),
    Location(Cmp, i64),
    Taxon(Cmp, i64),
    TaxonNameLike(String),
    User(i64),
}

impl FilterPart for Filter {
    fn add_to_query(&self, builder: &mut sqlx::QueryBuilder<sqlx::Sqlite>) {
        match self {
            Self::Sample(cmp, id) => _ = builder.push("S.id").push(cmp).push_bind(*id),
            Self::SampleNotIn(list) => {
                _ = builder.push("S.id NOT IN (");
                let mut sep = builder.separated(", ");
                for id in list {
                    sep.push_bind(*id);
                }
                builder.push(")");
            }
            Self::Location(cmp, id) => _ = builder.push("L.locid").push(cmp).push_bind(*id),
            Self::Taxon(cmp, id) => _ = builder.push("S.tsn").push(cmp).push_bind(*id),
            Self::TaxonNameLike(s) => {
                if !s.is_empty() {
                    let wildcard = format!("%{s}%");
                    builder.push(" (");
                    builder.push(" T.unit_name1 LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(" OR T.unit_name2 LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(" OR T.unit_name3 LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(" OR V.vernacular_name LIKE ");
                    builder.push_bind(wildcard.clone());
                    builder.push(") ");
                }
            }
            Self::User(id) => _ = builder.push("S.userid=").push_bind(*id),
        };
    }
}

impl Sample {
    fn build_query(filter: Option<Box<dyn FilterPart>>) -> QueryBuilder<'static, Sqlite> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new(
            r#"SELECT S.id, T.tsn, T.parent_tsn as parentid, L.locid, L.name as locname, T.complete_name,
        T.unit_name1, T.unit_name2, T.unit_name3, T.phylo_sort_seq as seq,
                    quantity, month, year, notes, certainty,
                    GROUP_CONCAT(V.vernacular_name, "@") as cnames,
                    U.id as userid, U.username
                    FROM sc_samples S
                    INNER JOIN taxonomic_units T ON T.tsn=S.tsn
                    INNER JOIN sc_locations L on L.locid=S.collectedlocation
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

    pub async fn fetch_all_user(
        userid: i64,
        filter: Option<Box<dyn FilterPart>>,
        pool: &Pool<Sqlite>,
    ) -> anyhow::Result<Vec<Sample>> {
        let mut newfilter = CompoundFilter::new(FilterOp::And);
        newfilter.add_filter(Box::new(Filter::User(userid)));
        if let Some(f) = filter {
            newfilter.add_filter(f);
        }
        let mut builder = Self::build_query(Some(Box::new(newfilter)));
        Ok(builder.build_query_as().fetch_all(pool).await?)
    }

    pub async fn fetch_all(
        filter: Option<Box<dyn FilterPart>>,
        pool: &Pool<Sqlite>,
    ) -> anyhow::Result<Vec<Sample>> {
        let mut builder = Self::build_query(filter);
        Ok(builder.build_query_as().fetch_all(pool).await?)
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> anyhow::Result<Sample> {
        let mut builder = Self::build_query(Some(Box::new(Filter::Sample(Cmp::Equal, id))));
        Ok(builder.build_query_as().fetch_one(pool).await?)
    }
}

impl FromRow<'_, SqliteRow> for Sample {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            id: row.try_get("id")?,
            user: User::from_row(row)?,
            taxon: Taxon::from_row(row)?,
            location: Location::from_row(row)?,
            quantity: row.try_get("quantity").unwrap_or(None),
            month: row.try_get("month").unwrap_or(None),
            year: row.try_get("year").unwrap_or(None),
            notes: row.try_get("notes").unwrap_or(None),
            certainty: row.try_get("certainty").unwrap_or(Certainty::Uncertain),
        })
    }
}

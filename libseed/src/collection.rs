use crate::sample::Sample;

#[derive(sqlx::FromRow)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(skip)]
    pub samples: Vec<Sample>,
}

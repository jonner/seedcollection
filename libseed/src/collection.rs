use crate::sample::Sample;
use serde::{Deserialize, Serialize};

#[derive(sqlx::FromRow, Deserialize, Serialize)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(skip)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<Sample>,
}

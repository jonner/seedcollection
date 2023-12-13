use serde::Deserialize;
use serde::Serialize;

#[derive(sqlx::FromRow, Deserialize, Serialize)]
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
}

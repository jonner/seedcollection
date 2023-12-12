#[derive(sqlx::FromRow)]
pub struct Location {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

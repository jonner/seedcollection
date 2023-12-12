#[derive(sqlx::FromRow)]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
}

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

const MAP_TILER_KEY: &'static str = "OfKZsQq0kXBWp83M3Wjx";
impl Location {
    pub fn map_viewer_uri(&self, zoom: f32) -> Option<String> {
        match (self.latitude, self.longitude) {
            (Some(latitude), Some(longitude)) => Some(format!("https://api.maptiler.com/maps/topo-v2/?key={MAP_TILER_KEY}#{zoom}/{latitude}/{longitude}")),
            _ => None,
        }
    }
}

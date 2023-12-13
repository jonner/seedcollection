use anyhow::Result;
use axum::{routing::get, Router};

mod db;
mod error;
mod location;
mod taxonomy;

#[tokio::main]
async fn main() -> Result<()> {
    let app = Router::new()
        .route("/", get(root))
        .nest("/location", location::router())
        .nest("/taxonomy", taxonomy::router());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> &'static str {
    "Welcome to seedweb"
}

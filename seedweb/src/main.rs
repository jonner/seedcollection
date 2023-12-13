use anyhow::Result;
use axum::{routing::get, Router};
use clap::Parser;
use std::sync::Arc;

mod collection;
mod db;
mod error;
mod location;
mod sample;
mod state;
mod taxonomy;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[arg(short, long, default_value = "seedcollection.sqlite")]
    pub database: String,
    #[arg(short, long, default_value = "localhost")]
    pub listen: String,
    #[arg(short, long, default_value = "3000")]
    pub port: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    let shared_state = Arc::new(state::SharedState::new(args.database).await?);

    let app = Router::new()
        .route("/", get(root))
        .nest("/collection", collection::router())
        .nest("/location", location::router())
        .nest("/sample", sample::router())
        .nest("/taxonomy", taxonomy::router())
        .with_state(shared_state);

    let addr = format!("{}:{}", args.listen, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> &'static str {
    "Welcome to seedweb"
}

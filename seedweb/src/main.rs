use anyhow::Result;
use axum::{
    async_trait,
    extract::{rejection::MatchedPathRejection, FromRequestParts, MatchedPath},
    http::request::Parts,
    response::{IntoResponse, Redirect},
    routing::get,
    RequestPartsExt, Router,
};
use axum_template::engine::Engine;
use clap::Parser;
use log::debug;
use minijinja::Environment;
use state::SharedState;
use tower_http::services::ServeDir;

mod api;
mod db;
mod error;
mod html;
mod state;

const APP_PREFIX: &str = "/app/";

pub fn logger() -> env_logger::Builder {
    let env = env_logger::Env::new()
        .filter_or("SW_LOG", "warn")
        .write_style("SW_LOG_STYLE");
    env_logger::Builder::from_env(env)
}

// Because minijinja loads an entire folder, we need to remove the `/` prefix
// and add a `.html` suffix. We can implement our own custom key extractor that
// transform the key
pub struct CustomKey(pub String);

#[async_trait]
impl<S> FromRequestParts<S> for CustomKey
where
    S: Send + Sync,
{
    type Rejection = MatchedPathRejection;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let mut key = parts
            .extract::<MatchedPath>()
            .await?
            // Cargo doesn't allow `:` as a file name
            .as_str()
            .trim_start_matches(APP_PREFIX)
            .replace(":", "$")
            .replace("/", "_");

        if key.is_empty() {
            key = "_INDEX".to_string();
        }
        key.push_str(".html");
        Ok(CustomKey(key))
    }
}

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
    logger().init();
    let args = Cli::parse();
    debug!("using database '{}'", args.database);

    let mut jinja = Environment::new();
    jinja.set_loader(minijinja::path_loader("seedweb/src/html/templates"));
    let shared_state = SharedState::new(args.database, Engine::from(jinja)).await?;

    let app = Router::new()
        .route("/", get(root))
        .route("/favicon.ico", get(favicon_redirect))
        .nest_service("/static", ServeDir::new("seedweb/src/html/static"))
        .nest("/app/", html::router())
        .nest("/api/v1/", api::router())
        .with_state(shared_state);

    let addr = format!("{}:{}", args.listen, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root() -> impl IntoResponse {
    Redirect::permanent(APP_PREFIX)
}

async fn favicon_redirect() -> impl IntoResponse {
    Redirect::permanent("/static/favicon.ico")
}

use anyhow::Result;
use axum::{
    async_trait,
    extract::{rejection::MatchedPathRejection, FromRequestParts, MatchedPath, State},
    http::request::Parts,
    response::IntoResponse,
    routing::get,
    RequestPartsExt, Router,
};
use axum_template::{engine::Engine, RenderHtml};
use clap::Parser;
use log::debug;
use minijinja::Environment;
use state::SharedState;

mod api;
mod db;
mod error;
mod html;
mod state;

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
        let key = parts
            // `axum_template::Key` internally uses `axum::extract::MatchedPath`
            .extract::<MatchedPath>()
            .await?
            .as_str()
            // Cargo doesn't allow `:` as a file name
            .replace(":", "$")
            .replace("/", "_")
            .chars()
            // Add the `.html` suffix
            .chain(".html".chars())
            .collect();
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
    let shared_state = state::SharedState::new(args.database, Engine::from(jinja)).await?;

    let app = Router::new()
        .route("/", get(root))
        .nest("/app/", html::router())
        .nest("/api/v1/", api::router())
        .with_state(shared_state);

    let addr = format!("{}:{}", args.listen, args.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("Listening on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root(CustomKey(key): CustomKey, State(state): State<SharedState>) -> impl IntoResponse {
    RenderHtml(key, state.tmpl, "")
}

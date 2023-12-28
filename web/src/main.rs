use anyhow::Result;
use axum::{
    async_trait,
    error_handling::HandleErrorLayer,
    extract::{rejection::MatchedPathRejection, FromRequestParts, MatchedPath},
    http::{request::Parts, StatusCode, Uri},
    response::{IntoResponse, Redirect},
    routing::get,
    BoxError, RequestPartsExt, Router,
};
use axum_login::{
    tower_sessions::{Expiry, SessionManagerLayer, SqliteStore},
    AuthManagerLayerBuilder,
};
use axum_template::engine::Engine;
use clap::Parser;
use log::debug;
use minijinja::{Environment, ErrorKind};
use serde::Serialize;
use state::SharedState;
use std::collections::HashMap;
use time::Duration;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;

mod api;
mod auth;
mod db;
mod error;
mod html;
mod state;

const API_PREFIX: &'static str = "/api/v1/";
const APP_PREFIX: &str = "/app/";

#[derive(Serialize)]
pub enum MessageType {
    Success,
    Warning,
    Error,
}

#[derive(Serialize)]
pub struct Message {
    r#type: MessageType,
    msg: String,
}

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

pub fn api_url(value: String) -> String {
    [API_PREFIX, &value.trim_start_matches('/')].join("")
}

pub fn app_url(value: String) -> String {
    [APP_PREFIX, &value.trim_start_matches('/')].join("")
}

pub fn append_query_param(
    uristr: String,
    key: String,
    value: String,
) -> Result<String, minijinja::Error> {
    let uri = uristr.parse::<Uri>().map_err(|e| {
        minijinja::Error::new(ErrorKind::InvalidOperation, "Unable to parse uri string")
            .with_source(e)
    })?;
    let mut query: HashMap<_, _> = match uri.query() {
        Some(q) => q.split("&").map(|s| s.split_once("=").unwrap()).collect(),
        None => HashMap::new(),
    };
    query.insert(key.as_str(), value.as_str());
    let querystring = query
        .drain()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    Ok(format!("?{querystring}"))
}

#[tokio::main]
async fn main() -> Result<()> {
    logger().init();
    let args = Cli::parse();
    debug!("using database '{}'", args.database);

    let mut jinja = Environment::new();
    jinja.set_loader(minijinja::path_loader("web/src/html/templates"));
    jinja.add_filter("app_url", app_url);
    jinja.add_filter("api_url", api_url);
    jinja.add_filter("append_query_param", append_query_param);

    let shared_state = SharedState::new(args.database, Engine::from(jinja)).await?;

    let session_store = SqliteStore::new(shared_state.dbpool.clone());
    session_store.migrate().await?;
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(Duration::days(1)));

    let auth_backend = auth::SqliteAuthBackend::new(shared_state.dbpool.clone());
    let auth_service = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_: BoxError| async {
            StatusCode::BAD_REQUEST
        }))
        .layer(AuthManagerLayerBuilder::new(auth_backend, session_layer).build());

    let app = Router::new()
        .route("/", get(root))
        .route("/favicon.ico", get(favicon_redirect))
        .nest_service("/static", ServeDir::new("web/src/html/static"))
        .nest(APP_PREFIX, html::router())
        .nest(API_PREFIX, api::router())
        .with_state(shared_state)
        .layer(auth_service);

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

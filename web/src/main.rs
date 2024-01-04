use anyhow::Result;
use axum::{
    async_trait,
    error_handling::HandleErrorLayer,
    extract::{rejection::MatchedPathRejection, FromRequestParts, Host, MatchedPath},
    handler::HandlerWithoutStateExt,
    http::{request::Parts, StatusCode, Uri},
    response::{IntoResponse, Redirect},
    routing::get,
    BoxError, RequestPartsExt, Router,
};
use axum_login::{
    tower_sessions::{Expiry, SessionManagerLayer, SqliteStore},
    AuthManagerLayerBuilder,
};
use axum_server::tls_rustls::RustlsConfig;
use axum_template::engine::Engine;
use clap::Parser;
use minijinja::{Environment, ErrorKind};
use serde::Serialize;
use state::SharedState;
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};
use time::Duration;
use tower::ServiceBuilder;
use tower_http::services::ServeDir;
use tracing::{debug, info};
use tracing_subscriber::filter::EnvFilter;

mod api;
mod auth;
mod db;
mod error;
mod html;
mod state;

const API_PREFIX: &str = "/api/v1/";
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
            .replace(':', "$")
            .replace('/', "_");

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
    #[arg(short, long, default_value = "8080")]
    pub port: u16,
    #[arg(short, long, default_value = "8443")]
    pub tls_port: u16,
}

pub fn api_url(value: String) -> String {
    [API_PREFIX, &value.trim_start_matches('/')].join("")
}

pub fn app_url(value: &str) -> String {
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
        Some(q) => q.split('&').map(|s| s.split_once('=').unwrap()).collect(),
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

pub fn truncate_text(mut s: String, chars: Option<usize>) -> String {
    let chars = chars.unwrap_or(100);
    if s.len() > chars {
        s.truncate(chars);
        s + "..."
    } else {
        s
    }
}

pub fn format_id_number(id: i64, width: Option<usize>) -> String {
    let width = width.unwrap_or(4);
    format!("{:0>width$}", id, width = width)
}

#[derive(Clone, Copy)]
struct Ports {
    http: u16,
    https: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_env("SEEDWEB_LOG"))
        .init();
    let args = Cli::parse();
    debug!("using database '{}'", args.database);

    let ports = Ports {
        http: args.port,
        https: args.tls_port,
    };

    tokio::spawn(redirect_http_to_https(args.listen.clone(), ports));

    let tlsconfig = RustlsConfig::from_pem_file(
        PathBuf::from("certs").join("server.crt"),
        PathBuf::from("certs").join("server.key"),
    )
    .await?;

    let mut jinja = Environment::new();
    jinja.set_loader(minijinja::path_loader("web/src/html/templates"));
    jinja.add_filter("app_url", app_url);
    jinja.add_filter("api_url", api_url);
    jinja.add_filter("append_query_param", append_query_param);
    jinja.add_filter("truncate", truncate_text);
    jinja.add_filter("idfmt", format_id_number);

    let shared_state = Arc::new(SharedState::new(args.database, Engine::from(jinja)).await?);
    sqlx::migrate!("../db/migrations")
        .run(&shared_state.dbpool)
        .await?;

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

    let addr: SocketAddr = format!("{}:{}", args.listen, ports.https).parse()?;
    info!("Listening on https://{}", addr);
    axum_server::bind_rustls(addr, tlsconfig)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

async fn redirect_http_to_https(addr: String, ports: Ports) {
    fn make_https(host: String, uri: Uri, ports: Ports) -> Result<Uri, BoxError> {
        let mut parts = uri.into_parts();

        parts.scheme = Some(axum::http::uri::Scheme::HTTPS);

        if parts.path_and_query.is_none() {
            parts.path_and_query = Some("/".parse().unwrap());
        }

        let https_host = host.replace(&ports.http.to_string(), &ports.https.to_string());
        parts.authority = Some(https_host.parse()?);

        Ok(Uri::from_parts(parts)?)
    }

    let redirect = move |Host(host): Host, uri: Uri| async move {
        match make_https(host, uri, ports) {
            Ok(uri) => Ok(Redirect::permanent(&uri.to_string())),
            Err(error) => {
                tracing::warn!(%error, "failed to convert URI to HTTPS");
                Err(StatusCode::BAD_REQUEST)
            }
        }
    };

    let addr: SocketAddr = format!("{}:{}", addr, ports.http).parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::debug!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, redirect.into_make_service())
        .await
        .unwrap();
}

async fn root() -> impl IntoResponse {
    Redirect::permanent(APP_PREFIX)
}

async fn favicon_redirect() -> impl IntoResponse {
    Redirect::permanent("/static/favicon.ico")
}

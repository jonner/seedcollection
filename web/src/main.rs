use crate::error::Error;
use anyhow::{anyhow, Context, Result};
use auth::AuthSession;
use axum::{
    async_trait,
    error_handling::HandleErrorLayer,
    extract::{rejection::MatchedPathRejection, FromRequestParts, Host, MatchedPath, State},
    handler::HandlerWithoutStateExt,
    http::{request::Parts, HeaderMap, HeaderValue, Method, Request, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    BoxError, RequestPartsExt, Router,
};
use axum_login::{
    tower_sessions::{Expiry, SessionManagerLayer, SqliteStore},
    AuthManagerLayerBuilder,
};
use axum_server::tls_rustls::RustlsConfig;
use axum_template::{engine::Engine, RenderHtml};
use clap::Parser;
use lettre::{transport::smtp::authentication::Credentials, AsyncSmtpTransport, Tokio1Executor};
use minijinja::{context, Environment, ErrorKind};
use serde::{Deserialize, Serialize};
use state::{AppState, SharedState};
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};
use time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestId, RequestId},
    services::ServeDir,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
    ServiceBuilderExt,
};
use tracing::{debug, info};
use tracing_subscriber::filter::EnvFilter;
use uuid::Uuid;

mod auth;
mod db;
mod error;
mod html;
mod state;

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
pub struct TemplateKey(pub String);

#[async_trait]
impl<S> FromRequestParts<S> for TemplateKey
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
            .replace(':', "@")
            .replace('/', "_");

        if key.is_empty() {
            key = "_INDEX".to_string();
        }
        if parts.method != Method::GET {
            key.push_str(&format!("-{}", parts.method));
        }
        key.push_str(".html");
        Ok(TemplateKey(key))
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    #[arg(long, required(true))]
    pub env: String,
    #[arg(
        long,
        exclusive(true),
        help = "shows all valid values for the --env option"
    )]
    pub list_envs: bool,
}

pub fn app_url(value: &str) -> String {
    [APP_PREFIX, &value.trim_start_matches('/')].join("")
}

pub fn markdown(value: Option<&str>) -> minijinja::Value {
    let value = value.unwrap_or("");
    let parser = pulldown_cmark::Parser::new(value);
    let mut output = String::new();
    pulldown_cmark::html::push_html(&mut output, parser);
    minijinja::Value::from_safe_string(output)
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
        Some(q) => serde_urlencoded::from_str(q).map_err(|e| {
            minijinja::Error::new(ErrorKind::InvalidOperation, "Unable to decode query params")
                .with_source(e)
        })?,
        None => HashMap::new(),
    };
    query.insert(key.as_str(), value.as_str());
    let querystring = serde_urlencoded::to_string(query).map_err(|e| {
        minijinja::Error::new(ErrorKind::InvalidOperation, "Unable to encode query params")
            .with_source(e)
    })?;

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

pub fn format_id_number(id: i64, prefix: Option<&str>, width: Option<usize>) -> String {
    let width = width.unwrap_or(4);
    let prefix = prefix.unwrap_or("");
    format!("{}{:0>width$}", prefix, id, width = width)
}

#[derive(Clone, Copy)]
struct Ports {
    http: u16,
    https: u16,
}

#[derive(Deserialize, PartialEq)]
struct RemoteSmtpCredentials {
    username: String,
    passwordfile: String,
    #[serde(skip)]
    password: String,
}

impl std::fmt::Debug for RemoteSmtpCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteSmtpCredentials")
            .field("username", &self.username)
            .field("passwordfile", &self.passwordfile)
            .finish()
    }
}

#[derive(Debug, Deserialize, PartialEq)]
struct RemoteSmtpConfig {
    url: String,
    credentials: Option<RemoteSmtpCredentials>,
    port: Option<u16>,
    timeout: Option<u64>,
}

impl RemoteSmtpConfig {
    fn build(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::from_url(&self.url)?;
        if let Some(ref creds) = self.credentials {
            builder = builder.credentials(Credentials::new(
                creds.username.clone(),
                creds.password.clone(),
            ));
        }
        if let Some(port) = self.port {
            builder = builder.port(port);
        }
        if let Some(timeout) = self.timeout {
            builder = builder.timeout(Some(std::time::Duration::new(timeout, 0)));
        }

        Ok(builder.build())
    }
}

#[derive(Deserialize)]
enum SmtpConfig {
    Local,
    Remote(RemoteSmtpConfig),
}

#[derive(Debug, Deserialize, PartialEq)]
enum MailTransport {
    File(String),
    LocalSmtp,
    Smtp(RemoteSmtpConfig),
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
struct ListenConfig {
    host: String,
    http_port: u16,
    https_port: u16,
}

#[derive(Debug, Deserialize, PartialEq)]
struct EnvConfig {
    listen: ListenConfig,
    database: String,
    asset_root: PathBuf,
    mail_transport: MailTransport,
}

impl EnvConfig {
    fn init(&mut self) -> Result<()> {
        if let MailTransport::Smtp(ref mut cfg) = self.mail_transport {
            if let Some(ref mut creds) = cfg.credentials {
                creds.password = std::fs::read_to_string(&creds.passwordfile)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
struct MakeRequestUuid;

impl MakeRequestId for MakeRequestUuid {
    fn make_request_id<B>(&mut self, _: &Request<B>) -> Option<RequestId> {
        let uuidstr = Uuid::new_v4().to_string();
        let headerval = HeaderValue::from_str(&uuidstr).ok()?;
        Some(RequestId::new(headerval))
    }
}

fn template_engine<T>(envname: &str, template_dir: T) -> Engine<minijinja::Environment<'static>>
where
    T: AsRef<std::path::Path>,
{
    let mut jinja = Environment::new();
    jinja.set_loader(minijinja::path_loader(template_dir));
    jinja.add_filter("app_url", app_url);
    jinja.add_filter("append_query_param", append_query_param);
    jinja.add_filter("truncate", truncate_text);
    jinja.add_filter("idfmt", format_id_number);
    jinja.add_filter("markdown", markdown);
    jinja.add_global("environment", envname);
    minijinja_contrib::add_to_environment(&mut jinja);

    Engine::from(jinja)
}

async fn app(shared_state: AppState) -> Result<Router> {
    sqlx::migrate!("../db/migrations")
        .run(&shared_state.dbpool)
        .await?;

    debug!("Creating session layer");
    let session_store = SqliteStore::new(shared_state.dbpool.clone());
    session_store.migrate().await?;
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(true)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)));

    debug!("Creating auth backend");
    let auth_backend = auth::SqliteAuthBackend::new(shared_state.dbpool.clone());
    let auth_service = ServiceBuilder::new()
        .layer(HandleErrorLayer::new(|_: BoxError| async {
            StatusCode::BAD_REQUEST
        }))
        .layer(AuthManagerLayerBuilder::new(auth_backend, session_layer).build());

    debug!("Creating routers");
    let static_path = shared_state.config.asset_root.join("static");
    let app = Router::new()
        .route("/", get(root))
        .route("/favicon.ico", get(favicon_redirect))
        .nest_service("/static", ServeDir::new(static_path))
        .nest(APP_PREFIX, html::router(shared_state.clone()))
        .layer(
            ServiceBuilder::new()
                .set_x_request_id(MakeRequestUuid::default())
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(DefaultMakeSpan::new().include_headers(true))
                        .on_response(DefaultOnResponse::new().include_headers(true)),
                )
                .propagate_x_request_id()
                .layer(auth_service)
                .layer(middleware::from_fn_with_state(
                    shared_state.clone(),
                    error_mapper,
                )),
        )
        .with_state(shared_state);

    Ok(app)
}

#[tokio::main]
async fn main() -> Result<()> {
    let configyaml = tokio::fs::read_to_string("config.yaml").await?;
    let mut configs: HashMap<String, EnvConfig> = serde_yaml::from_str(&configyaml)?;
    let args = Cli::parse();

    if args.list_envs {
        for key in configs.keys() {
            println!("{key}");
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_env("SEEDWEB_LOG"))
        .init();

    let mut env = configs.remove(&args.env).ok_or_else(|| {
        anyhow!(
            "Unknown environment '{}'. Possible values: {}",
            &args.env,
            configs.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })?;
    // we want to fail early if the config isn't valid or the password can't be read
    env.init()?;
    info!("Using environment '{}'", args.env);
    let listen = env.listen.clone();

    let ports = Ports {
        http: listen.http_port,
        https: listen.https_port,
    };

    tokio::spawn(redirect_http_to_https(listen.host.clone(), ports));

    let tlsconfig = RustlsConfig::from_pem_file(
        PathBuf::from("certs").join("server.crt"),
        PathBuf::from("certs").join("server.key"),
    )
    .await
    .with_context(|| "Unable to load TLS key and certificate. See certs/README for more info")?;

    let app = app(Arc::new(SharedState::new(&args.env, env).await?)).await?;

    let addr: SocketAddr = format!("{}:{}", listen.host, listen.https_port).parse()?;
    info!("Listening on https://{}", addr);
    axum_server::bind_rustls(addr, tlsconfig)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

async fn error_mapper(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let is_htmx = headers.get("HX-Request").is_some();
    let response = next.run(request).await;
    if is_htmx {
        // don't print out a fancy error page for HTMX since it will just get inserted inside a
        // section of the page and look weird.
        return response;
    }

    let server_error = response.extensions().get::<Arc<Error>>();
    let client_status = server_error.map(|se| se.as_ref().to_client_status());

    let error_response = client_status.as_ref().map(|(status_code, client_error)| {
        (
            *status_code,
            RenderHtml(
                "_ERROR.html",
                state.tmpl.clone(),
                context!(status_code => status_code.as_u16(),
                status_reason => status_code.canonical_reason(),
                client_error => client_error,
                user => auth.user,
                request_id => headers.get("x-request-id").map(|h| h.to_str().unwrap_or("")),
                ),
            ),
        )
            .into_response()
    });

    error_response.unwrap_or(response)
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

async fn favicon_redirect(State(state): State<AppState>) -> impl IntoResponse {
    let path = state.config.asset_root.join("static/favicon.ico");
    Redirect::permanent(path.to_str().unwrap_or_default())
}

#[cfg(test)]
async fn test_app(pool: sqlx::Pool<sqlx::Sqlite>) -> Result<Router> {
    let state = Arc::new(SharedState::test(pool));
    app(state).await
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_config() {
        let yaml = r#"dev:
  database: dev-database.sqlite
  asset_root: "/path/to/assets"
  mail_transport: !File
    "/tmp/"
  listen: !ListenConfig &LISTEN
    host: "0.0.0.0"
    http_port: 8080
    https_port: 8443
prod:
  database: prod-database.sqlite
  mail_transport: !LocalSmtp
  asset_root: "/path/to/assets2"
  listen: *LISTEN"#;
        let configs: HashMap<String, EnvConfig> =
            serde_yaml::from_str(yaml).expect("Failed to parse yaml");
        assert_eq!(configs.len(), 2);
        assert_eq!(
            configs["dev"],
            EnvConfig {
                asset_root: PathBuf::from("/path/to/assets"),
                database: "dev-database.sqlite".to_string(),
                mail_transport: MailTransport::File("/tmp/".to_string()),
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    http_port: 8080,
                    https_port: 8443,
                }
            }
        );
        assert_eq!(
            configs["prod"],
            EnvConfig {
                asset_root: PathBuf::from("/path/to/assets2"),
                database: "prod-database.sqlite".to_string(),
                mail_transport: MailTransport::LocalSmtp,
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    http_port: 8080,
                    https_port: 8443,
                }
            }
        );
    }
}

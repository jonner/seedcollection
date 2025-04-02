use crate::error::Error;
use anyhow::{Context, Result, anyhow};
use auth::AuthSession;
use axum::{
    BoxError, RequestPartsExt, Router,
    extract::{FromRequestParts, MatchedPath, State, rejection::MatchedPathRejection},
    handler::HandlerWithoutStateExt,
    http::{HeaderMap, HeaderValue, Method, Request, StatusCode, Uri, request::Parts},
    middleware::{self, Next},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use axum_extra::extract::Host;
use axum_login::{
    AuthManagerLayerBuilder,
    tower_sessions::{Expiry, SessionManagerLayer},
};
use axum_server::tls_rustls::RustlsConfig;
use axum_template::{RenderHtml, engine::Engine};
use clap::Parser;
use lettre::{AsyncSmtpTransport, Tokio1Executor, transport::smtp::authentication::Credentials};
use minijinja::{Environment, context};
use serde::Deserialize;
use state::{AppState, SharedState};
use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};
use time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    ServiceBuilderExt,
    request_id::{MakeRequestId, RequestId},
    services::ServeDir,
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tower_sessions_sqlx_store::SqliteStore;
use tracing::{debug, info, trace};
use tracing_subscriber::filter::EnvFilter;
use uuid::Uuid;

mod auth;
mod error;
mod html;
mod state;
mod util;

const APP_PREFIX: &str = "/app/";

// Because minijinja loads an entire folder, we need to remove the `/` prefix
// and add a `.html.j2` suffix. We can implement our own custom key extractor that
// transform the key
pub(crate) struct TemplateKey(pub(crate) String);

impl<S> FromRequestParts<S> for TemplateKey
where
    S: Send + Sync,
{
    type Rejection = MatchedPathRejection;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let mp = parts.extract::<MatchedPath>().await?;
        Ok(TemplateKey(path_to_template_key(
            mp.as_str(),
            &parts.method,
        )))
    }
}

fn path_to_template_key(path: &str, method: &Method) -> String {
    // Cargo doesn't allow `:` as a file name
    let mut key: String = path
        .trim_start_matches(APP_PREFIX)
        .trim_start_matches('/')
        .chars()
        .map(|c| match c {
            ':' => '@',
            '/' => '_',
            _ => c,
        })
        .collect();

    if key.is_empty() {
        key = "_INDEX".to_string();
    }
    if method != Method::GET {
        key.push_str(&format!("-{}", method));
    }
    key.push_str(".html.j2");
    key
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub(crate) struct Cli {
    #[arg(
        long,
        help = "The path to a configuration directory. This directory should contain at minimum a 'config.yaml' file which defines a set of runtime environments."
    )]
    pub(crate) configdir: Option<PathBuf>,
    #[arg(
        long,
        required_unless_present("list_envs"),
        help = "The name of the runtime environment to execute. This should be the name of an environment defined in 'config.yaml'"
    )]
    pub(crate) env: Option<String>,
    #[arg(
        long,
        conflicts_with("env"),
        help = "shows all valid values for the --env option"
    )]
    pub(crate) list_envs: bool,
}

#[derive(Debug, Clone, Copy)]
struct Ports {
    http: u16,
    https: u16,
}

#[derive(Deserialize, PartialEq)]
struct RemoteSmtpCredentials {
    username: String,
    #[serde(default)]
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
    mail_transport: MailTransport,
}

impl EnvConfig {
    fn init(&mut self) -> Result<()> {
        if let MailTransport::Smtp(ref mut cfg) = self.mail_transport {
            if let Some(ref mut creds) = cfg.credentials {
                // 'passwordfile' entry in environment config takes priority
                if !creds.passwordfile.is_empty() {
                    debug!(
                        "Looking up SMTP password from file '{}'",
                        creds.passwordfile
                    );
                    creds.password =
                        std::fs::read_to_string(&creds.passwordfile).with_context(|| {
                            format!(
                                "Failed to read smtp password from file '{}'",
                                creds.passwordfile
                            )
                        })?;
                } else {
                    debug!("Looking up SMTP password from environment variable");
                    // If not found, look it up from environment variable
                    creds.password = std::env::var("SEEDWEB_SMTP_PASSWORD").with_context(
                        || "Failed to get SMTP password from env variable SEEDWEB_SMTP_PASSWORD",
                    )?;
                }
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

fn template_engine<'source, T>(
    envname: &str,
    template_dir: T,
) -> Engine<minijinja::Environment<'source>>
where
    T: AsRef<std::path::Path>,
{
    let mut jinja = Environment::new();
    jinja.set_loader(minijinja::path_loader(template_dir));
    jinja.add_filter("app_url", util::app_url);
    jinja.add_filter("append_query_param", util::append_query_param);
    jinja.add_filter("idfmt", util::format_id_number);
    jinja.add_filter("markdown", util::markdown);
    jinja.add_filter("qtyfmt", util::format_quantity);
    jinja.add_global("environment", envname);
    minijinja_contrib::add_to_environment(&mut jinja);

    Engine::from(jinja)
}

async fn app(shared_state: AppState) -> Result<Router> {
    trace!("Creating session layer");
    let session_store = SqliteStore::new(shared_state.db.pool().clone());
    session_store.migrate().await?;
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(true)
        .with_expiry(Expiry::OnInactivity(Duration::days(7)));

    trace!("Creating auth backend");
    let auth_backend = auth::SqliteAuthBackend::new(shared_state.db.clone());
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    trace!("Creating routers");
    let static_path = shared_state.datadir.join("static");
    if !static_path.join("js").exists() {
        return Err(Error::Environment(format!("The `js` directory does not exist in `{static_path:?}`. You may need to install javascript packages (with e.g. `yarn install`) and copy them to the correct location.")).into());
    }

    let app = Router::new()
        .route("/", get(root))
        .route("/favicon.ico", get(favicon_redirect))
        .nest_service("/static", ServeDir::new(static_path))
        .nest(APP_PREFIX, html::router(shared_state.clone()))
        .layer(
            ServiceBuilder::new()
                .set_x_request_id(MakeRequestUuid)
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(DefaultMakeSpan::new().include_headers(true))
                        .on_response(DefaultOnResponse::new().include_headers(true)),
                )
                .propagate_x_request_id()
                .layer(auth_layer)
                .layer(middleware::from_fn_with_state(
                    shared_state.clone(),
                    error_mapper,
                )),
        )
        .with_state(shared_state);

    Ok(app)
}

fn config_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("org", "quotidian", "seedweb").ok_or_else(|| {
        Error::Environment("Failed to determine base directories for configuration".to_string())
    })?;

    let testdir = dirs.config_dir();
    debug!(?testdir, "checking config dir");
    if testdir.exists() {
        return Ok(testdir.to_path_buf());
    }

    // on unix, fall back to  systemwide config dir
    #[cfg(unix)]
    {
        Ok(PathBuf::from("/etc/seedweb"))
    }
    #[cfg(not(unix))]
    {
        Err(anyhow!("Couldn't determine config directory"))
    }
}

fn data_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("org", "quotidian", "seedweb").ok_or_else(|| {
        Error::Environment("Failed to determine base directories for configuration".to_string())
    })?;
    let testdir = dirs.data_dir();
    debug!(?testdir, "checking data dir");
    if testdir.exists() {
        return Ok(testdir.to_path_buf());
    }
    // on unix, fall back to system data dir
    #[cfg(unix)]
    {
        Ok(PathBuf::from("/usr/share/seedweb"))
    }
    #[cfg(not(unix))]
    {
        Err(anyhow!("Couldn't determine data directory"))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_env("SEEDWEB_LOG"))
        .init();

    let args = Cli::parse();
    let configdir = match args.configdir {
        Some(dir) => dir,
        None => config_dir()?,
    };
    debug!(?configdir, "Configuration directory");
    let datadir = data_dir()?;
    debug!(?datadir, "Data directory");

    let configfile = configdir.join("config.yaml");
    let configyaml = tokio::fs::read_to_string(&configfile)
        .await
        .with_context(|| format!("Couldn't read configuration file {:?}", &configfile))?;
    let mut configs: HashMap<String, EnvConfig> = serde_yaml::from_str(&configyaml)?;

    if args.list_envs {
        for key in configs.keys() {
            println!("{key}");
        }
        return Ok(());
    }

    let envarg = args.env.as_ref().expect("'env' argument required");
    let mut env = configs.remove(envarg).ok_or_else(|| {
        anyhow!(
            "Unknown environment '{}'. Possible values: {}",
            envarg,
            configs.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    })?;
    // we want to fail early if the config isn't valid or the password can't be read
    env.init()?;
    info!(envarg, ?env);
    let listen = env.listen.clone();

    let ports = Ports {
        http: listen.http_port,
        https: listen.https_port,
    };

    tokio::spawn(redirect_http_to_https(listen.host.clone(), ports));

    let certdir = configdir.join("certs");
    let tlsconfig =
        RustlsConfig::from_pem_file(certdir.join("server.crt"), certdir.join("server.key"))
            .await
            .with_context(
                || "Unable to load TLS key and certificate. See certs/README for more info",
            )?;

    let app = app(Arc::new(SharedState::new(envarg, env, datadir).await?)).await?;

    let handle = axum_server::Handle::new();
    #[cfg(unix)]
    tokio::spawn(shutdown_on_sigterm(handle.clone()));
    let addr: SocketAddr = format!("{}:{}", listen.host, listen.https_port).parse()?;
    info!("Listening on https://{}", addr);
    axum_server::bind_rustls(addr, tlsconfig)
        .handle(handle)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

#[cfg(unix)]
async fn shutdown_on_sigterm(handle: axum_server::Handle) {
    use tokio::signal::unix::*;
    signal(SignalKind::terminate())
        .expect("Failed to install signal handler")
        .recv()
        .await;
    handle.graceful_shutdown(None);
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
                "_ERROR.html.j2",
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
        info!("Redirecting {uri:?} to https");
        match make_https(host, uri, ports) {
            Ok(uri) => Ok(Redirect::permanent(&uri.to_string())),
            Err(error) => {
                tracing::warn!(%error, "failed to convert URI to HTTPS");
                Err(StatusCode::BAD_REQUEST)
            }
        }
    };

    let addr: SocketAddr = format!("{}:{}", addr, ports.http).parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|_| panic!("Failed to listen on address {addr:?}"));
    info!(
        "Redirector listening on http://{}",
        listener.local_addr().unwrap()
    );
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

    #[test]
    fn test_template_key() {
        struct Case {
            path: String,
            method: Method,
            expected: &'static str,
        }
        let cases = [
            Case {
                path: "".to_owned(),
                method: Method::GET,
                expected: "_INDEX.html.j2",
            },
            Case {
                path: "/".to_owned(),
                method: Method::GET,
                expected: "_INDEX.html.j2",
            },
            Case {
                path: APP_PREFIX.to_owned(),
                method: Method::GET,
                expected: "_INDEX.html.j2",
            },
            Case {
                path: APP_PREFIX.to_owned() + "/foo",
                method: Method::GET,
                expected: "foo.html.j2",
            },
            Case {
                path: APP_PREFIX.to_owned() + "foo",
                method: Method::GET,
                expected: "foo.html.j2",
            },
            Case {
                path: APP_PREFIX.to_owned() + "foo/bar",
                method: Method::GET,
                expected: "foo_bar.html.j2",
            },
            Case {
                path: APP_PREFIX.to_owned() + "foo/bar",
                method: Method::PUT,
                expected: "foo_bar-PUT.html.j2",
            },
            Case {
                path: APP_PREFIX.to_owned() + "foo/{bar}",
                method: Method::GET,
                expected: "foo_{bar}.html.j2",
            },
            Case {
                path: APP_PREFIX.to_owned() + "foo/{bar}",
                method: Method::PUT,
                expected: "foo_{bar}-PUT.html.j2",
            },
        ];
        for case in cases {
            assert_eq!(
                path_to_template_key(&case.path, &case.method),
                case.expected
            );
        }
    }
}

use anyhow::{Context, Result, anyhow};
use axum::http::Uri;
use lettre::{AsyncSmtpTransport, Tokio1Executor, transport::smtp::authentication::Credentials};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer};
use tracing::debug;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteSmtpCredentials {
    pub(crate) username: String,
    #[serde(default)]
    pub(crate) passwordfile: String,
    #[serde(skip)]
    pub(crate) password: SecretString,
}

impl PartialEq for RemoteSmtpCredentials {
    fn eq(&self, other: &Self) -> bool {
        self.username == other.username
            && self.passwordfile == other.passwordfile
            && self.password.expose_secret() == other.password.expose_secret()
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteSmtpConfig {
    pub(crate) url: String,
    pub(crate) credentials: Option<RemoteSmtpCredentials>,
    pub(crate) port: Option<u16>,
    pub(crate) timeout: Option<u64>,
}

impl RemoteSmtpConfig {
    pub(crate) fn build(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::from_url(&self.url)?;
        if let Some(ref creds) = self.credentials {
            builder = builder.credentials(Credentials::new(
                creds.username.clone(),
                creds.password.expose_secret().to_string(),
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
#[serde(deny_unknown_fields)]
pub(crate) enum MailTransport {
    File(String),
    LocalSmtp,
    Smtp(RemoteSmtpConfig),
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListenConfig {
    pub(crate) host: String,
    pub(crate) port: u16,
}

const DEFAULT_HTTP_PORT: u16 = 80;
const DEFAULT_HOST: &str = "0.0.0.0";
fn default_listen() -> ListenConfig {
    ListenConfig {
        host: DEFAULT_HOST.to_string(),
        port: DEFAULT_HTTP_PORT,
    }
}

// This handles the case where the `listen` block is PRESENT, but a field may be missing.
fn deserialize_listen_with_default_port<'de, D>(deserializer: D) -> Result<ListenConfig, D::Error>
where
    D: Deserializer<'de>,
{
    // Define a helper struct that mirrors ListenConfig but with an optional port or host.
    #[derive(Deserialize)]
    struct PartialListenConfig {
        host: Option<String>,
        port: Option<u16>,
    }

    // Deserialize the data into our helper struct.
    let partial_config = PartialListenConfig::deserialize(deserializer)?;

    // Create the final ListenConfig, supplying the default port if it was missing.
    Ok(ListenConfig {
        host: partial_config
            .host
            .unwrap_or_else(|| DEFAULT_HOST.to_string()),
        port: partial_config.port.unwrap_or(DEFAULT_HTTP_PORT),
    })
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct EnvConfig {
    #[serde(default = "default_listen")]
    #[serde(deserialize_with = "deserialize_listen_with_default_port")]
    pub(crate) listen: ListenConfig,
    pub(crate) database: String,
    pub(crate) mail_transport: MailTransport,
    #[serde(default)]
    pub(crate) user_registration_enabled: bool,
    #[serde(with = "http_serde::uri")]
    pub(crate) public_address: Uri,
    pub(crate) metrics: Option<ListenConfig>,
}

impl EnvConfig {
    pub(crate) fn init(&mut self) -> Result<()> {
        if let MailTransport::Smtp(ref mut cfg) = self.mail_transport
            && let Some(ref mut creds) = cfg.credentials
        {
            // 'passwordfile' entry in environment config takes priority
            if !creds.passwordfile.is_empty() {
                debug!(
                    "Looking up SMTP password from file '{}'",
                    creds.passwordfile
                );
                creds.password = std::fs::read_to_string(&creds.passwordfile)
                    .with_context(|| {
                        format!(
                            "Failed to read smtp password from file '{}'",
                            creds.passwordfile
                        )
                    })?
                    .into();
            } else {
                debug!("Looking up SMTP password from environment variable");
                // If not found, look it up from environment variable
                creds.password = std::env::var("SEEDWEB_SMTP_PASSWORD")
                    .with_context(
                        || "Failed to get SMTP password from env variable SEEDWEB_SMTP_PASSWORD",
                    )?
                    .into();
            }
        }
        if self.public_address.scheme().is_none() {
            return Err(anyhow!("public_address must specify a scheme (e.g. https)"));
        }
        if self.public_address.authority().is_none() {
            return Err(anyhow!("public_address must specify a host name"));
        }
        if let Some(query) = self.public_address.query() {
            return Err(anyhow!("ignoring query string '{query}' in public_address"));
        }
        match self.public_address.path() {
            "" | "/" => (),
            p if !p.ends_with('/') => {
                return Err(anyhow!(
                    "public address path must end with a trailing '/' character"
                ));
            }
            _ => (),
        }
        Ok(())
    }

    pub(crate) fn prefix(&self) -> Option<&str> {
        match self.public_address.path() {
            "" | "/" => None,
            p => Some(p),
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_parse_config() {
        let yaml = r#"dev:
  database: dev-database.sqlite
  mail_transport: !File "/tmp/"
  listen: &LISTEN
    host: "0.0.0.0"
    port: 8080
  public_address: "http://dev.server.com"
test:
  database: prod-database.sqlite
  mail_transport: !LocalSmtp
  listen: *LISTEN
  public_address: "http://test.server.com/app"
prod:
  database: prod-database.sqlite
  mail_transport: !Smtp
    url: "smtp.example.com"
    credentials:
      username: "user123"
      passwordfile: "/path/to/passwordfile"
    port: 25
    timeout: 61
  public_address: "https://prod.server.com"
  listen: *LISTEN"#;
        let configs: HashMap<String, EnvConfig> =
            serde_yaml::from_str(yaml).expect("Failed to parse yaml");
        assert_eq!(configs.len(), 3);
        assert_eq!(
            configs["dev"],
            EnvConfig {
                database: "dev-database.sqlite".to_string(),
                mail_transport: MailTransport::File("/tmp/".to_string()),
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                },
                user_registration_enabled: false,
                public_address: Uri::from_static("http://dev.server.com"),
                metrics: None,
            }
        );
        assert_eq!(
            configs["test"],
            EnvConfig {
                database: "prod-database.sqlite".to_string(),
                mail_transport: MailTransport::LocalSmtp,
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                },
                user_registration_enabled: false,
                public_address: Uri::from_static("http://test.server.com/app"),
                metrics: None,
            }
        );
        assert_eq!(
            configs["prod"],
            EnvConfig {
                database: "prod-database.sqlite".to_string(),
                mail_transport: MailTransport::Smtp(RemoteSmtpConfig {
                    url: "smtp.example.com".into(),
                    credentials: Some(RemoteSmtpCredentials {
                        username: "user123".into(),
                        passwordfile: "/path/to/passwordfile".into(),
                        password: Default::default()
                    }),
                    port: Some(25),
                    timeout: Some(61)
                }),
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                },
                user_registration_enabled: false,
                public_address: Uri::from_static("https://prod.server.com"),
                metrics: None,
            }
        );
    }

    #[test]
    fn test_default_ports() {
        let yaml = r#"dev:
  database: dev-database.sqlite
  mail_transport: !File "/tmp/"
  listen:
    host: "0.0.0.0"
  public_address: "http://dev.server.com""#;
        let configs: HashMap<String, EnvConfig> =
            serde_yaml::from_str(yaml).expect("Failed to parse yaml");
        assert_eq!(configs["dev"].listen.port, 80);
    }

    #[test]
    fn test_public_address() {
        let parse = |addr: &str| {
            let yaml = format!(
                r#"dev:
  database: dev-database.sqlite
  mail_transport: !File "/tmp/"
  listen:
    host: "0.0.0.0"
  public_address: "{addr}""#
            );

            serde_yaml::from_str::<HashMap<String, EnvConfig>>(&yaml)
        };

        assert!(parse("https:///").is_err());
        assert!(parse("https://").is_err());
        assert!(parse("example.com/").is_err());
        assert!(parse("//example.com/").is_ok());
        assert!(parse("http://example.com/").is_ok());

        let addr_config = |addr| {
            let yaml = format!(
                r#"dev:
  database: dev-database.sqlite
  mail_transport: !File "/tmp/"
  listen:
    host: "0.0.0.0"
  public_address: "{addr}""#
            );
            let mut configs: HashMap<String, EnvConfig> =
                serde_yaml::from_str(&yaml).expect("Failed to parse yaml");
            configs.remove("dev").expect("didn't find 'dev' config")
        };

        // basic address
        assert!(addr_config("https://example.com/").init().is_ok());
        // missing scheme
        assert!(addr_config("//example.com/").init().is_err());
        // has path with trailing slash
        assert!(addr_config("https://example.com/foo/").init().is_ok());
        // has path without trailing slash
        assert!(addr_config("https://example.com/foo").init().is_err());
        // has query string
        assert!(
            addr_config("https://example.com/foo/?foo=1")
                .init()
                .is_err()
        );
    }
}

use anyhow::{Context, Result};
use lettre::{AsyncSmtpTransport, Tokio1Executor, transport::smtp::authentication::Credentials};
use serde::Deserialize;
use tracing::debug;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Ports {
    pub(crate) http: u16,
    pub(crate) https: u16,
}

#[derive(Deserialize, PartialEq)]
pub(crate) struct RemoteSmtpCredentials {
    pub(crate) username: String,
    #[serde(default)]
    pub(crate) passwordfile: String,
    #[serde(skip)]
    pub(crate) password: String,
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
pub(crate) enum MailTransport {
    File(String),
    LocalSmtp,
    Smtp(RemoteSmtpConfig),
}

fn default_http_port() -> u16 {
    80
}

fn default_https_port() -> u16 {
    443
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub(crate) struct ListenConfig {
    pub(crate) host: String,
    #[serde(default = "default_http_port")]
    pub(crate) http_port: u16,
    #[serde(default = "default_https_port")]
    pub(crate) https_port: u16,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct EnvConfig {
    pub(crate) listen: ListenConfig,
    pub(crate) database: String,
    pub(crate) mail_transport: MailTransport,
    #[serde(default)]
    pub(crate) user_registration_enabled: bool,
    pub(crate) public_base_url: String,
}

impl EnvConfig {
    pub(crate) fn init(&mut self) -> Result<()> {
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

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn test_parse_config() {
        let yaml = r#"dev:
  database: dev-database.sqlite
  asset_root: "/path/to/assets"
  mail_transport: !File "/tmp/"
  listen: &LISTEN
    host: "0.0.0.0"
    http_port: 8080
    https_port: 8443
  public_base_url: "http://dev.server.com"
test:
  database: prod-database.sqlite
  mail_transport: !LocalSmtp
  asset_root: "/path/to/assets2"
  listen: *LISTEN
  public_base_url: "http://test.server.com"
prod:
  database: prod-database.sqlite
  mail_transport: !Smtp
    url: "smtp.example.com"
    credentials:
      username: "user123"
      passwordfile: "/path/to/passwordfile"
    port: 25
    timeout: 61
  public_base_url: "https://prod.server.com"
  asset_root: "/path/to/assets2"
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
                    http_port: 8080,
                    https_port: 8443,
                },
                user_registration_enabled: false,
                public_base_url: "http://dev.server.com".into(),
            }
        );
        assert_eq!(
            configs["test"],
            EnvConfig {
                database: "prod-database.sqlite".to_string(),
                mail_transport: MailTransport::LocalSmtp,
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    http_port: 8080,
                    https_port: 8443,
                },
                user_registration_enabled: false,
                public_base_url: "http://test.server.com".into(),
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
                        password: String::new()
                    }),
                    port: Some(25),
                    timeout: Some(61)
                }),
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    http_port: 8080,
                    https_port: 8443,
                },
                user_registration_enabled: false,
                public_base_url: "https://prod.server.com".into(),
            }
        );
    }

    #[test]
    fn test_default_ports() {
        let yaml = r#"dev:
  database: dev-database.sqlite
  asset_root: "/path/to/assets"
  mail_transport: !File "/tmp/"
  listen:
    host: "0.0.0.0"
  public_base_url: "http://dev.server.com""#;
        let configs: HashMap<String, EnvConfig> =
            serde_yaml::from_str(yaml).expect("Failed to parse yaml");
        assert_eq!(configs["dev"].listen.http_port, 80);
        assert_eq!(configs["dev"].listen.https_port, 443);
    }
}

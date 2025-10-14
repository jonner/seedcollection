use anyhow::{Context, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer};
use tracing::debug;

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteSmtpConfig {
    pub(crate) url: String,
    pub(crate) credentials: Option<RemoteSmtpCredentials>,
    pub(crate) port: Option<u16>,
    pub(crate) timeout: Option<u64>,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct MailSender {
    pub(crate) name: String,
    pub(crate) address: String,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(deny_unknown_fields)]
pub(crate) struct MailService {
    pub(crate) transport: MailTransport,
    pub(crate) sender: MailSender,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
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
    pub(crate) mail_service: MailService,
    #[serde(default)]
    pub(crate) user_registration_enabled: bool,
    pub(crate) public_base_url: String,
    pub(crate) metrics: Option<ListenConfig>,
}

impl EnvConfig {
    pub(crate) fn init(&mut self) -> Result<()> {
        if let MailTransport::Smtp(ref mut cfg) = self.mail_service.transport
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
  mail_service:
    transport: !File "/tmp/"
    sender: !MailSender
      name: "SeedCollection"
      address: "nobody@example.com"
  listen: &LISTEN
    host: "0.0.0.0"
    port: 8080
  public_base_url: "http://dev.server.com"
test:
  database: prod-database.sqlite
  mail_service: !MailService
    transport: !LocalSmtp
    sender: !MailSender
      name: "SeedCollection"
      address: "nobody@example.com"
  listen: *LISTEN
  public_base_url: "http://test.server.com"
prod:
  database: prod-database.sqlite
  mail_service: !MailService
    transport: !Smtp
      url: "smtp.example.com"
      credentials:
        username: "user123"
        passwordfile: "/path/to/passwordfile"
      port: 25
      timeout: 61
    sender: !MailSender
        name: "SeedCollection"
        address: "nobody@example.com"
  public_base_url: "https://prod.server.com"
  listen: *LISTEN"#;
        let configs: HashMap<String, EnvConfig> =
            serde_yaml::from_str(yaml).expect("Failed to parse yaml");
        assert_eq!(configs.len(), 3);
        assert_eq!(
            configs["dev"],
            EnvConfig {
                database: "dev-database.sqlite".to_string(),
                mail_service: MailService {
                    transport: MailTransport::File("/tmp/".to_string()),
                    sender: MailSender {
                        name: "SeedCollection".to_string(),
                        address: "nobody@example.com".to_string(),
                    }
                },
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                },
                user_registration_enabled: false,
                public_base_url: "http://dev.server.com".into(),
                metrics: None,
            }
        );
        assert_eq!(
            configs["test"],
            EnvConfig {
                database: "prod-database.sqlite".to_string(),
                mail_service: MailService {
                    transport: MailTransport::LocalSmtp,
                    sender: MailSender {
                        name: "SeedCollection".to_string(),
                        address: "nobody@example.com".to_string(),
                    }
                },
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                },
                user_registration_enabled: false,
                public_base_url: "http://test.server.com".into(),
                metrics: None,
            }
        );
        assert_eq!(
            configs["prod"],
            EnvConfig {
                database: "prod-database.sqlite".to_string(),
                mail_service: MailService {
                    transport: MailTransport::Smtp(RemoteSmtpConfig {
                        url: "smtp.example.com".into(),
                        credentials: Some(RemoteSmtpCredentials {
                            username: "user123".into(),
                            passwordfile: "/path/to/passwordfile".into(),
                            password: Default::default()
                        }),
                        port: Some(25),
                        timeout: Some(61)
                    }),
                    sender: MailSender {
                        name: "SeedCollection".to_string(),
                        address: "nobody@example.com".to_string(),
                    }
                },
                listen: ListenConfig {
                    host: "0.0.0.0".to_string(),
                    port: 8080,
                },
                user_registration_enabled: false,
                public_base_url: "https://prod.server.com".into(),
                metrics: None,
            }
        );
    }

    #[test]
    fn test_default_ports() {
        let yaml = r#"dev:
  database: dev-database.sqlite
  mail_service: !MailService
    transport: !File "/tmp/"
    sender: !MailSender
      name: "SeedCollection"
      address: "nobody@example.com"
  listen:
    host: "0.0.0.0"
  public_base_url: "http://dev.server.com""#;
        let configs: HashMap<String, EnvConfig> =
            serde_yaml::from_str(yaml).expect("Failed to parse yaml");
        assert_eq!(configs["dev"].listen.port, 80);
    }
}

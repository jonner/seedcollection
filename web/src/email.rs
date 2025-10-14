use std::time::Duration;

use crate::{
    Result,
    config::{self, MailService},
};

use lettre::{
    Address, AsyncFileTransport, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::Mailbox, transport::smtp::authentication::Credentials,
};
use secrecy::ExposeSecret;
use tracing::debug;

#[derive(Debug)]
pub enum AnyAsyncTransport {
    File(AsyncFileTransport<Tokio1Executor>),
    Smtp(AsyncSmtpTransport<Tokio1Executor>),
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Generic(#[from] lettre::error::Error),
    #[error(transparent)]
    FileTranport(#[from] lettre::transport::file::Error),
    #[error(transparent)]
    SmtpTransport(#[from] lettre::transport::smtp::Error),
}

impl AnyAsyncTransport {
    pub fn from_config(cfg: &config::MailTransport) -> Result<Self, Error> {
        Ok(match cfg {
            config::MailTransport::File(p) => Self::File(AsyncFileTransport::new(p.clone())),
            config::MailTransport::LocalSmtp => {
                Self::Smtp(AsyncSmtpTransport::<Tokio1Executor>::unencrypted_localhost())
            }
            config::MailTransport::Smtp(cfg) => {
                let mut transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(&cfg.url)?;
                if let Some(c) = &cfg.credentials {
                    let creds = Credentials::new(
                        c.username.clone(),
                        c.password.expose_secret().to_string(),
                    );
                    transport = transport.credentials(creds);
                }
                if let Some(port) = cfg.port {
                    transport = transport.port(port);
                }
                transport = transport.timeout(cfg.timeout.map(Duration::from_secs));
                Self::Smtp(transport.build())
            }
        })
    }

    #[tracing::instrument(ret)]
    pub async fn test_connection(&self) -> Result<(), Error> {
        debug!("Testing connection");
        match self {
            AnyAsyncTransport::File(_) => Ok(()), // Nothing to test for file transport
            AnyAsyncTransport::Smtp(t) => {
                t.test_connection().await.map(|_| ()).map_err(|e| e.into())
            }
        }
    }
}

#[async_trait::async_trait]
impl AsyncTransport for AnyAsyncTransport {
    type Ok = ();
    type Error = Error;

    async fn send_raw(
        &self,
        envelope: &lettre::address::Envelope,
        email: &[u8],
    ) -> Result<Self::Ok, Self::Error> {
        match self {
            AnyAsyncTransport::File(t) => t
                .send_raw(envelope, email)
                .await
                .map_err(Into::into)
                .map(|_| ()),
            AnyAsyncTransport::Smtp(t) => t
                .send_raw(envelope, email)
                .await
                .map_err(Into::into)
                .map(|_| ()),
        }
    }
}

#[derive(Debug)]
pub struct EmailService {
    transport: AnyAsyncTransport,
    from: Mailbox,
}

impl EmailService {
    pub async fn new(cfg: &MailService) -> Result<Self> {
        let from = Mailbox::new(
            Some(cfg.sender.name.clone()),
            cfg.sender.address.parse::<Address>()?,
        );
        let transport = AnyAsyncTransport::from_config(&cfg.transport)?;
        transport.test_connection().await?;
        Ok(Self { transport, from })
    }

    pub async fn send(&self, to: Mailbox, subject: String, body: String) -> Result<(), Error> {
        let message = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(subject)
            .body(body)?;

        self.transport.send(message).await
    }
}

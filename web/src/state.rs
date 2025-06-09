use crate::{
    config::{EnvConfig, MailTransport},
    error::Error,
    template_engine,
    util::{FlashMessage, app_url},
};
use anyhow::{Context, Result};
use axum::response::IntoResponse;
use axum_template::{RenderHtml, TemplateEngine, engine::Engine};
use lettre::{
    AsyncFileTransport, AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
    message::{Mailbox, header::ContentType},
};
use libseed::{core::database::Database, user::verification::UserVerification};
use minijinja::context;
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tracing::{debug, trace};

#[derive(Debug)]
pub(crate) struct SharedState {
    pub(crate) db: Database,
    pub(crate) tmpl: Engine<minijinja::Environment<'static>>,
    pub(crate) config: EnvConfig,
    pub(crate) datadir: PathBuf,
}

impl SharedState {
    pub(crate) async fn new(envname: &str, env: EnvConfig, datadir: PathBuf) -> Result<Self> {
        let tmpl_path = datadir.join("templates");
        let template = template_engine(envname, &tmpl_path);
        trace!("Creating shared app state");
        // do a quick sanity check on the mail transport
        debug!(?env.mail_transport,
            "Sanity checking the mail transport",
        );
        match env.mail_transport {
            crate::config::MailTransport::File(_) => Ok(()),
            crate::config::MailTransport::LocalSmtp => {
                let t = AsyncSmtpTransport::<Tokio1Executor>::unencrypted_localhost();
                t.test_connection().await.map(|_| ())
            }
            crate::config::MailTransport::Smtp(ref cfg) => {
                let t = cfg.build()?;
                t.test_connection().await.map(|_| ())
            }
        }
        .with_context(|| "Sanity check of mail transport failed")?;
        Ok(Self {
            db: Database::open(env.database.clone())
                .await
                .with_context(|| format!("Unable to open database {}", &env.database))?,
            tmpl: template,
            config: env,
            datadir,
        })
    }

    async fn create_verification_email(
        &self,
        uv: &mut UserVerification,
    ) -> Result<lettre::Message, Error> {
        let verification_url = self.user_verification_url(uv);
        let user = uv.user.load(&self.db, false).await?;
        let emailbody = self
            .tmpl
            .render(
                "verification-email",
                context!(user => user,
                     verification_url => verification_url),
            )
            .with_context(|| "Failed to render email")?;
        let email = lettre::Message::builder()
            .from(
                "NOBODY <jonathon@quotidian.org>"
                    .parse()
                    .with_context(|| "failed to parse sender address")?,
            )
            .to(Mailbox::new(
                user.display_name.clone(),
                user.email
                    .parse()
                    .with_context(|| "Failed to parse recipient address")?,
            ))
            .subject("Verify your email address")
            .header(ContentType::TEXT_PLAIN)
            .body(emailbody)
            .with_context(|| "Failed to create email message")?;
        Ok(email)
    }

    fn user_verification_url(&self, uv: &UserVerification) -> String {
        let path = format!("/auth/verify/{}/{}", uv.user.id(), uv.key);
        self.public_url(&path)
    }

    fn public_url(&self, path: &str) -> String {
        let mut public_url = self
            .config
            .public_base_url
            .trim_end_matches('/')
            .to_string();
        public_url.push_str(&app_url(path));
        public_url
    }

    pub async fn send_verification(&self, mut uv: UserVerification) -> Result<(), Error> {
        let email = self.create_verification_email(&mut uv).await?;
        match self.config.mail_transport {
            MailTransport::File(ref path) => AsyncFileTransport::<Tokio1Executor>::new(path)
                .send(email)
                .await
                .map_err(anyhow::Error::from)
                .map(|_| ()),
            MailTransport::LocalSmtp => {
                AsyncSmtpTransport::<Tokio1Executor>::unencrypted_localhost()
                    .send(email)
                    .await
                    .map_err(anyhow::Error::from)
                    .map(|_| ())
            }
            MailTransport::Smtp(ref cfg) => cfg
                .build()?
                .send(email)
                .await
                .map_err(anyhow::Error::from)
                .map(|_| ()),
        }
        .with_context(|| "Failed to send verification email")
        .map_err(|e| e.into())
    }

    pub fn render_template<'a, K, S>(
        self: Arc<Self>,
        template_key: K,
        data: S,
    ) -> RenderHtml<K, Engine<minijinja::Environment<'a>>, S>
    where
        S: Serialize,
    {
        RenderHtml(template_key, self.tmpl.clone(), data)
    }

    pub(crate) fn render_flash_message(self: Arc<Self>, msg: FlashMessage) -> impl IntoResponse {
        self.render_template("_flash_messages.html.j2", context!(messages => &[msg]))
    }

    #[cfg(test)]
    pub(crate) fn test(pool: sqlx::Pool<sqlx::Sqlite>) -> Self {
        use crate::config::ListenConfig;

        let template = template_engine("test", "./templates");
        debug!("Creating test shared app state");
        Self {
            db: Database::from(pool),
            tmpl: template,
            config: EnvConfig {
                listen: ListenConfig {
                    host: "127.0.0.1".to_string(),
                    port: 8080,
                },
                database: "test-database.sqlite".to_string(),
                mail_transport: MailTransport::File("/tmp/".to_string()),
                user_registration_enabled: false,
                public_base_url: "http://test.com".into(),
                metrics: None,
            },
            datadir: ".".into(),
        }
    }
}

pub(crate) type AppState = Arc<SharedState>;

#[cfg(test)]
mod test {
    use super::*;

    #[sqlx::test]
    fn test_verification_url(pool: sqlx::Pool<sqlx::Sqlite>) {
        let mut state = SharedState::test(pool);
        const USERID: i64 = 14;
        const VERIFICATION_KEY: &str = "asdf09asnwdflaksdflisudf";
        let uv = UserVerification {
            id: 12,
            user: libseed::core::loadable::ExternalRef::Stub(USERID),
            key: VERIFICATION_KEY.into(),
            requested: None,
            expiration: 24,
            confirmed: false,
        };
        let expected_path = app_url(&format!("/auth/verify/{USERID}/{VERIFICATION_KEY}"));
        // make sure that there will be a path separator between the base url
        // and the application path
        assert_eq!(
            expected_path
                .chars()
                .next()
                .expect("Couldn't get first character"),
            '/'
        );

        state.config.public_base_url = "http://test.com".to_string();
        let url = state.user_verification_url(&uv);
        assert_eq!(url, format!("http://test.com{expected_path}"));

        // test with trailing '/' in base url
        state.config.public_base_url = "http://test.com/".to_string();
        let url = state.user_verification_url(&uv);
        assert_eq!(url, format!("http://test.com{expected_path}"));

        // test with trailing path in base url
        state.config.public_base_url = "https://test.com/foo/".to_string();
        let url = state.user_verification_url(&uv);
        assert_eq!(url, format!("https://test.com/foo{expected_path}"));
    }
}

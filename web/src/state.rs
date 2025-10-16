use crate::{
    config::EnvConfig, email::EmailService, error::Error, template_engine, util::FlashMessage,
};
use anyhow::{Context, Result};
use axum::response::IntoResponse;
use axum_template::{RenderHtml, TemplateEngine, engine::Engine};
use lettre::message::Mailbox;
use libseed::{core::database::Database, user::verification::UserVerification};
use minijinja::context;
use serde::Serialize;
use std::{path::PathBuf, sync::Arc};
use tracing::trace;

#[derive(Debug)]
pub(crate) struct SharedState {
    pub(crate) db: Database,
    pub(crate) tmpl: Engine<minijinja::Environment<'static>>,
    pub(crate) config: EnvConfig,
    pub(crate) datadir: PathBuf,
    pub(crate) email_service: EmailService,
}

impl SharedState {
    pub(crate) async fn new(envname: &str, env: EnvConfig, datadir: PathBuf) -> Result<Self> {
        let tmpl_path = datadir.join("templates");
        let template = template_engine(
            &tmpl_path,
            env.public_address.path().to_string(),
            // FIXME: in the future, perhaps the yaml configuration can contain
            // a setting to indicate that the environment is a production
            // environment
            envname == "prod",
        );
        trace!("Creating shared app state");

        Ok(Self {
            db: Database::open(env.database.clone())
                .await
                .with_context(|| format!("Unable to open database {}", &env.database))?,
            tmpl: template,
            email_service: EmailService::new(&env.mail_service).await?,
            config: env,
            datadir,
        })
    }

    fn user_verification_url(&self, uv: &UserVerification) -> String {
        let path = format!("/auth/verify/{}/{}", uv.user.id(), uv.key);
        self.public_url(&path)
    }

    pub(crate) fn path(&self, path: &str) -> String {
        let prefix = self.config.public_address.path();
        let mut url = prefix.to_string();
        if !url.ends_with('/') {
            url.push('/');
        }
        url.push_str(path.trim_start_matches('/'));
        url
    }

    fn public_url(&self, path: &str) -> String {
        let mut public_url = self.config.public_address.to_string();
        if !public_url.ends_with('/') {
            public_url.push('/');
        }
        public_url.push_str(path.trim_start_matches('/'));
        public_url
    }

    pub async fn send_verification(&self, mut uv: UserVerification) -> Result<(), Error> {
        let verification_url = self.user_verification_url(&uv);
        let user = uv.user.load(&self.db, false).await?;
        let emailbody = self
            .tmpl
            .render(
                "verification-email",
                context!(
                    user => user,
                    verification_url => verification_url,
                ),
            )
            .with_context(|| "Failed to render email")?;

        let to = Mailbox::new(
            user.display_name.clone(),
            user.email
                .parse()
                .with_context(|| "Failed to parse recipient address")?,
        );

        self.email_service
            .send(to, "Verify your email address".to_string(), emailbody)
            .await
            .map_err(Into::into)
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

    #[tracing::instrument(ret)]
    pub(crate) fn render_flash_message(self: Arc<Self>, msg: FlashMessage) -> impl IntoResponse {
        self.render_template("_flash_messages.html.j2", context!(messages => &[msg]))
    }

    #[cfg(test)]
    pub(crate) async fn test(pool: sqlx::Pool<sqlx::Sqlite>) -> Self {
        use axum::http::Uri;
        use tracing::debug;

        use crate::config::{ListenConfig, MailSender, MailService, MailTransport};

        debug!("Creating test shared app state");
        let config = EnvConfig {
            listen: ListenConfig {
                host: "127.0.0.1".to_string(),
                port: 8080,
            },
            database: "test-database.sqlite".to_string(),
            mail_service: MailService {
                transport: MailTransport::File("/tmp/".to_string()),
                sender: MailSender {
                    name: "SeedCollection".to_string(),
                    address: "nobody@example.com".to_string(),
                },
            },
            user_registration_enabled: false,
            public_address: Uri::from_static("http://test.com/test/"),
            metrics: None,
        };
        let template = template_engine(
            "./templates",
            config.public_address.path().to_string(),
            false,
        );
        Self {
            db: Database::from(pool),
            tmpl: template,
            email_service: EmailService::new(&config.mail_service)
                .await
                .expect("Failed to create email service"),
            config,
            datadir: ".".into(),
        }
    }
}

pub(crate) type AppState = Arc<SharedState>;

#[cfg(test)]
mod test {
    use axum::http::Uri;

    use super::*;

    #[sqlx::test]
    async fn test_verification_url(pool: sqlx::Pool<sqlx::Sqlite>) {
        let mut state = SharedState::test(pool).await;
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
        let expected_path = &format!("/auth/verify/{USERID}/{VERIFICATION_KEY}");
        // make sure that there will be a path separator between the base url
        // and the application path
        assert_eq!(
            expected_path
                .chars()
                .next()
                .expect("Couldn't get first character"),
            '/'
        );

        state.config.public_address = Uri::from_static("http://test.com");
        let url = state.user_verification_url(&uv);
        assert_eq!(url, format!("http://test.com{expected_path}"));

        // test with trailing '/' in base url
        state.config.public_address = Uri::from_static("https://test.com/");
        let url = state.user_verification_url(&uv);
        assert_eq!(url, format!("https://test.com{expected_path}"));

        // test with trailing path
        state.config.public_address = Uri::from_static("https://test.com/foo/");
        let url = state.user_verification_url(&uv);
        assert_eq!(url, format!("https://test.com/foo{expected_path}"));
    }
}

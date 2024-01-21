use crate::error::{self, Error};
use anyhow::anyhow;
use axum::{async_trait, extract::FromRequestParts, http::request::Parts};
use axum_login::{AuthUser, AuthnBackend, UserId};
use libseed::{
    empty_string_as_none,
    user::{User, UserStatus},
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::ops::Deref;

#[derive(Debug, Clone, Serialize)]
pub struct SqliteUser(User);

impl Deref for SqliteUser {
    type Target = User;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AuthUser for SqliteUser {
    type Id = String;

    fn id(&self) -> Self::Id {
        self.username.clone()
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.pwhash.as_bytes()
    }
}

#[derive(Clone, Deserialize)]
pub struct Credentials {
    pub username: String,
    pub password: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub next: Option<String>,
}

#[derive(Clone)]
pub struct SqliteAuthBackend {
    db: SqlitePool,
}

#[async_trait]
impl AuthnBackend for SqliteAuthBackend {
    type User = SqliteUser;
    type Credentials = Credentials;
    type Error = Error;

    async fn authenticate(
        &self,
        credentials: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        tracing::info!("authenticating...");
        let user = self.get_user(&credentials.username).await?;
        tracing::info!("Got user {user:?}");
        match user {
            Some(user) => user
                .verify_password(&credentials.password)
                .map(|_| Some(user))
                .map_err(|e| e.into()),
            None => Ok(None),
        }
    }

    async fn get_user(&self, username: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        User::fetch_by_username(username, &self.db)
            .await
            .map(|o| o.map(|u| SqliteUser(u)))
            .map_err(|e| e.into())
    }
}

impl SqliteAuthBackend {
    pub async fn register(
        &self,
        username: String,
        email: String,
        password: String,
    ) -> Result<(), error::Error> {
        let password_hash = User::hash_password(&password)?;
        let mut user = User::new(
            username,
            email,
            password_hash,
            UserStatus::Unverified,
            None,
            None,
            None,
        );
        user.insert(&self.db).await?;
        Ok(())
    }

    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }
}

pub type AuthSession = axum_login::AuthSession<SqliteAuthBackend>;

#[async_trait]
impl<S> FromRequestParts<S> for SqliteUser
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let auth = AuthSession::from_request_parts(parts, _state)
            .await
            .map_err(|e| anyhow!(e.1))?;
        auth.user
            .ok_or_else(|| Error::Unauthorized("No logged in user".to_string()))
    }
}

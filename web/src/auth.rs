use crate::error::{self, Error};
use anyhow::anyhow;
use async_trait::async_trait;
use axum::{extract::FromRequestParts, http::request::Parts};
use axum_login::{AuthUser, AuthnBackend, UserId};
use libseed::{
    core::database::Database,
    empty_string_as_none,
    user::{User, UserStatus},
};
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SqliteUser(User);

impl From<SqliteUser> for User {
    fn from(value: SqliteUser) -> Self {
        value.0
    }
}

impl From<User> for SqliteUser {
    fn from(value: User) -> Self {
        Self(value)
    }
}

impl Deref for SqliteUser {
    type Target = User;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SqliteUser {
    fn deref_mut(&mut self) -> &mut User {
        &mut self.0
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

#[derive(Clone, Deserialize, Serialize)]
pub(crate) struct Credentials {
    pub(crate) username: String,
    pub(crate) password: String,
    #[serde(deserialize_with = "empty_string_as_none")]
    pub(crate) next: Option<String>,
}

#[derive(Clone)]
pub(crate) struct SqliteAuthBackend {
    db: Database,
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
        tracing::info!(?user, "Got user");
        match user {
            Some(user) => user
                .verify_password(&credentials.password)
                .map(|_| Some(user))
                .map_err(|e| e.into()),
            None => Ok(None),
        }
    }

    async fn get_user(&self, username: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        User::load_by_username(username, &self.db)
            .await
            .map(|o| o.map(SqliteUser))
            .map_err(|e| e.into())
    }
}

impl SqliteAuthBackend {
    pub(crate) async fn register(
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

    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }
}

pub(crate) type AuthSession = axum_login::AuthSession<SqliteAuthBackend>;

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

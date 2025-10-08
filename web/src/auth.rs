use crate::error::Error;
use anyhow::anyhow;
use axum::{extract::FromRequestParts, http::request::Parts};
use axum_login::{AuthUser, AuthnBackend, UserId};
use libseed::{core::database::Database, empty_string_as_none, user::User};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Serialize)]
/// A 'newtype' object that wraps [User] in order to implement the
/// [AuthUser] trait. It is intended to be used interchangeably
/// with the underlying object type
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

#[derive(Clone, Deserialize)]
/// Form fields that are submitted in order to log into the web application
pub(crate) struct Credentials {
    pub(crate) username: String,
    pub(crate) password: SecretString,
    /// An optional uri to redirect the user to after a successful login.
    #[serde(deserialize_with = "empty_string_as_none")]
    pub(crate) next: Option<String>,
}

#[derive(Clone)]
/// An authentication backend that uses an sqlite database to authenticate users
pub(crate) struct SqliteAuthBackend {
    db: Database,
}

#[derive(thiserror::Error, Debug)]
/// Potential errors that can occur when authenticating with the web application
pub(crate) enum AuthError {
    #[error(transparent)]
    Libseed(#[from] libseed::Error),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

impl AuthnBackend for SqliteAuthBackend {
    type User = SqliteUser;
    type Credentials = Credentials;
    type Error = AuthError;

    async fn authenticate(
        &self,
        credentials: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        tracing::info!("authenticating...");
        let user = self.get_user(&credentials.username).await?;
        tracing::info!(?user, "Got user");
        match user {
            Some(user) => user
                .verify_password(credentials.password.expose_secret())
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
    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }
}

/// An authentication session
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

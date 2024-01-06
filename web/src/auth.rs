use crate::error::{self, Error};
use anyhow::anyhow;
use argon2::Argon2;
use axum::{async_trait, extract::FromRequestParts, http::request::Parts};
use axum_login::{AuthUser, AuthnBackend, UserId};
use libseed::empty_string_as_none;
use password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize)]
pub struct SqliteUser {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pwhash: String,
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
        let user = self.get_user(&credentials.username).await?;
        match user {
            Some(user) => {
                let hasher = Argon2::default();
                let expected_hash = PasswordHash::new(&user.pwhash).map_err(Error::from)?;
                hasher
                    .verify_password(credentials.password.as_bytes(), &expected_hash)
                    .map(|_| Some(user))
                    .map_err(|e| e.into())
            }
            None => Ok(None),
        }
    }

    async fn get_user(&self, username: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        Ok(sqlx::query_as!(
            SqliteUser,
            "SELECT * from sc_users WHERE username=?",
            username
        )
        .fetch_optional(&self.db)
        .await?)
    }
}

impl SqliteAuthBackend {
    pub async fn register(&self, username: String, password: String) -> Result<(), error::Error> {
        let salt = SaltString::generate(&mut OsRng);
        let hasher = Argon2::default();
        let password_hash = hasher
            .hash_password(password.as_bytes(), &salt)?
            .to_string();
        sqlx::query!(
            "INSERT INTO sc_users (username, pwhash) VALUES (?, ?)",
            username,
            password_hash
        )
        .execute(&self.db)
        .await?;
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

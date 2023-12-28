use crate::error;
use argon2::Argon2;
use axum::async_trait;
use axum_login::{AuthUser, AuthnBackend, UserId};
use password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize)]
pub struct SqliteUser {
    id: i64,
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
}

#[derive(thiserror::Error, Debug)]
pub enum AuthenticationError {
    #[error("password error")]
    HashFailure(#[from] password_hash::errors::Error),
    #[error("Database error")]
    DatabaseError(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct SqliteAuthBackend {
    db: SqlitePool,
}

#[async_trait]
impl AuthnBackend for SqliteAuthBackend {
    type User = SqliteUser;
    type Credentials = Credentials;
    type Error = AuthenticationError;

    async fn authenticate(
        &self,
        credentials: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user = self.get_user(&credentials.username).await?;
        match user {
            Some(user) => {
                let hasher = Argon2::default();
                let expected_hash =
                    PasswordHash::new(&user.pwhash).map_err(|e| AuthenticationError::from(e))?;
                hasher
                    .verify_password(credentials.password.as_bytes(), &expected_hash)
                    .map(|_| Some(user))
                    .map_err(|e| e.into())
            }
            None => Ok(None),
        }
    }

    async fn get_user(&self, username: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        Ok(
            sqlx::query_as!(SqliteUser, "SELECT * from users WHERE username=?", username)
                .fetch_optional(&self.db)
                .await?,
        )
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
            "INSERT INTO users (username, pwhash) VALUES (?, ?)",
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

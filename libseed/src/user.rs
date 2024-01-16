use anyhow::anyhow;
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use async_trait::async_trait;
use password_hash::{rand_core::OsRng, PasswordHash, SaltString};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteQueryResult, Pool, Sqlite};

use crate::loadable::Loadable;

#[derive(sqlx::FromRow, Debug, Clone, Serialize, Deserialize)]
pub struct User {
    #[sqlx(rename = "userid")]
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    #[sqlx(default)]
    pub pwhash: String,
    #[serde(skip_serializing)]
    #[sqlx(skip)]
    loaded: bool,
}

#[async_trait]
impl Loadable for User {
    type Id = i64;

    fn is_loaded(&self) -> bool {
        self.loaded
    }

    fn is_loadable(&self) -> bool {
        self.id > 0
    }

    async fn do_load(&mut self, pool: &Pool<Sqlite>) -> anyhow::Result<Self> {
        User::fetch(self.id, pool).await
    }

    fn new_loadable(id: Self::Id) -> Self {
        User {
            id,
            username: Default::default(),
            pwhash: Default::default(),
            loaded: false,
        }
    }
}

impl User {
    pub async fn fetch_all(pool: &Pool<Sqlite>) -> anyhow::Result<Vec<User>> {
        Ok(sqlx::query_as(
            "SELECT id as userid, username, pwhash FROM sc_users ORDER BY username ASC",
        )
        .fetch_all(pool)
        .await
        .and_then(|mut v| {
            let _ = v.iter_mut().map(|u: &mut User| {
                u.loaded = true;
                u
            });
            Ok(v)
        })?)
    }

    pub async fn fetch(id: i64, pool: &Pool<Sqlite>) -> anyhow::Result<User> {
        Ok(
            sqlx::query_as("SELECT id as userid, username, pwhash FROM sc_users WHERE id=?")
                .bind(id)
                .fetch_one(pool)
                .await
                .and_then(|mut u: Self| {
                    u.loaded = true;
                    Ok(u)
                })?,
        )
    }

    pub async fn fetch_by_username(
        username: &str,
        pool: &Pool<Sqlite>,
    ) -> anyhow::Result<Option<User>> {
        sqlx::query_as("SELECT id as userid, username, pwhash FROM sc_users WHERE username=?")
            .bind(username)
            .fetch_optional(pool)
            .await
            .and_then(|opt| {
                Ok(opt.map(|mut u: User| {
                    u.loaded = true;
                    u
                }))
            })
            .map_err(|e| e.into())
    }

    pub async fn update(&self, pool: &Pool<Sqlite>) -> anyhow::Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(anyhow!("No id set, cannot update"));
        }

        sqlx::query("UPDATE sc_users SET username=?, pwhash=? WHERE id=?")
            .bind(&self.username)
            .bind(&self.pwhash)
            .bind(self.id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
    }

    pub async fn delete(&mut self, pool: &Pool<Sqlite>) -> anyhow::Result<SqliteQueryResult> {
        if self.id < 0 {
            return Err(anyhow!("No id set, cannot delete"));
        }

        sqlx::query("DELETE FROM sc_users WHERE id=?")
            .bind(self.id)
            .execute(pool)
            .await
            .map_err(|e| e.into())
            .and_then(|x| {
                self.id = -1;
                Ok(x)
            })
    }

    pub fn hash_password(pw: &str) -> anyhow::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let hasher = Argon2::default();
        Ok(hasher.hash_password(pw.as_bytes(), &salt)?.to_string())
    }

    pub fn verify_password(&self, pw: &str) -> anyhow::Result<()> {
        let hasher = Argon2::default();
        let expected_hash = PasswordHash::new(&self.pwhash)?;
        hasher
            .verify_password(pw.as_bytes(), &expected_hash)
            .map_err(|e| e.into())
    }

    pub fn change_password(&mut self, pw: &str) -> anyhow::Result<()> {
        self.pwhash = Self::hash_password(pw)?;
        Ok(())
    }

    pub fn new(username: String, pwhash: String) -> Self {
        Self {
            id: -1,
            username,
            pwhash,
            loaded: false,
        }
    }

    pub async fn insert(&self, pool: &Pool<Sqlite>) -> anyhow::Result<SqliteQueryResult> {
        sqlx::query("INSERT INTO sc_users (username, pwhash) VALUES (?, ?)")
            .bind(&self.username)
            .bind(&self.pwhash)
            .execute(pool)
            .await
            .map_err(|e| e.into())
    }
}

impl Default for User {
    fn default() -> Self {
        Self {
            id: -1,
            username: Default::default(),
            pwhash: Default::default(),
            loaded: Default::default(),
        }
    }
}

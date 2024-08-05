use anyhow::{Context, Result};
use libseed::user::User;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Sqlite, SqlitePool};
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use tokio::{
    fs::{read_to_string, set_permissions, File},
    io::AsyncWriteExt,
};
use tracing::debug;

#[derive(Deserialize, Serialize)]
pub struct Config {
    pub username: String,
    pub password: String,
    pub database: PathBuf,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Not Logged in")]
    NotLoggedIn,
    #[error("Failed to parse config file")]
    ConfigParseFailed(#[from] serde_json::Error),
    #[error("Incorrect username or password")]
    LoginFailure,
    #[error("Unable to connect to database")]
    DatabaseConnectionFailure,
    #[error("Unable to run database migrations")]
    DatabaseMigrationFailure,
    #[error(transparent)]
    Database(#[from] libseed::Error),
}

impl Config {
    fn parse(contents: String) -> Result<Self, serde_json::Error> {
        serde_json::from_str(&contents)
    }

    fn format(&self) -> Result<String> {
        serde_json::to_string_pretty(self).with_context(|| "Couldn't convert config to json")
    }

    pub async fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let p = path.as_ref();
        debug!(?p, "Trying to load login config");
        let contents = read_to_string(path).await.map_err(|_| Error::NotLoggedIn)?;
        Self::parse(contents).map_err(Error::ConfigParseFailed)
    }

    pub async fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        debug!(?path, "Saving login config");
        let mut file = File::create(path).await?;
        let serialized = self.format()?;
        let mut perms = file.metadata().await?.permissions();
        perms.set_mode(0o600);
        set_permissions(path, perms).await?;
        file.write_all(serialized.as_bytes())
            .await
            .with_context(|| "Failed to write config file")?;
        Ok(())
    }

    pub fn new(username: String, password: String, database: PathBuf) -> Self {
        Config {
            username,
            password,
            database,
        }
    }

    pub async fn validate(&self) -> Result<(Pool<Sqlite>, User), Error> {
        let dbpool = SqlitePool::connect(&format!("sqlite://{}", self.database.to_string_lossy()))
            .await
            .map_err(|_| Error::DatabaseConnectionFailure)?;
        sqlx::migrate!("../db/migrations")
            .run(&dbpool)
            .await
            .map_err(|_| Error::DatabaseMigrationFailure)?;
        let user = User::load_by_username(&self.username, &dbpool)
            .await
            .map_err(Error::Database)?
            .ok_or_else(|| Error::LoginFailure)?;
        user.verify_password(&self.password)
            .map_err(|_| Error::LoginFailure)?;
        Ok((dbpool, user))
    }
}

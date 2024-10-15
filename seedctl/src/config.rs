use libseed::{user::User, Database};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::{
    fs::{read_to_string, File},
    io::AsyncWriteExt,
};
use tracing::debug;
#[cfg(unix)]
use {std::os::unix::fs::PermissionsExt, tokio::fs::set_permissions};

#[derive(Deserialize, Serialize)]
pub(crate) struct Config {
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) database: PathBuf,
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Not Logged in")]
    NotLoggedIn,
    #[error("Failed to parse config file")]
    ConfigParseFailed(#[source] serde_json::Error),
    #[error("Incorrect username or password")]
    LoginFailure,
    #[error("Unable to run database migrations")]
    DatabaseMigrationFailure(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Database(#[from] libseed::Error),
    #[error("Failed to format config in JSON")]
    CannotFormatConfig(#[source] serde_json::Error),
    #[error("File permissions error for '{}': {1}", .0.to_string_lossy())]
    FilePermissions(PathBuf, &'static str, #[source] std::io::Error),
}

impl Config {
    fn parse(contents: String) -> Result<Self, serde_json::Error> {
        serde_json::from_str(&contents)
    }

    fn format(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(self).map_err(Error::CannotFormatConfig)
    }

    pub(crate) async fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let p = path.as_ref();
        debug!(?p, "Trying to load login config");
        let contents = read_to_string(path).await.map_err(|_| Error::NotLoggedIn)?;
        Self::parse(contents).map_err(Error::ConfigParseFailed)
    }

    pub(crate) async fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let path = path.as_ref();
        debug!(?path, "Saving login config");
        let mut file = File::create(path)
            .await
            .map_err(|e| Error::FilePermissions(path.to_owned(), "Creating file", e))?;
        let serialized = self.format()?;
        #[cfg(unix)]
        {
            let mut perms = file
                .metadata()
                .await
                .map_err(|e| Error::FilePermissions(path.to_owned(), "Querying metadata", e))?
                .permissions();
            perms.set_mode(0o600);
            set_permissions(path, perms)
                .await
                .map_err(|e| Error::FilePermissions(path.to_owned(), "Setting permissions", e))?;
        }
        file.write_all(serialized.as_bytes())
            .await
            .map_err(|e| Error::FilePermissions(path.to_owned(), "Writing file", e))
    }

    pub(crate) fn new(username: String, password: String, database: PathBuf) -> Self {
        Config {
            username,
            password,
            database,
        }
    }

    pub(crate) async fn validate(&self) -> Result<(Database, User), Error> {
        let db = libseed::Database::open(&self.database).await?;
        let user = User::load_by_username(&self.username, &db)
            .await
            .map_err(Error::Database)?
            .ok_or_else(|| Error::LoginFailure)?;
        user.verify_password(&self.password)
            .map_err(|_| Error::LoginFailure)?;
        Ok((db, user))
    }
}

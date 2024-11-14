//! functions related to `seedctl` configuration
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

/// A struct containing configuration required to successfully run `seedctl`
#[derive(Deserialize, Serialize)]
pub(crate) struct Config {
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) database: PathBuf,
}

/// Errors related to configuration
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
    #[error("File permissions error for '{path}': {1}", path = .0.to_string_lossy())]
    FilePermissions(PathBuf, &'static str, #[source] std::io::Error),
}

impl Config {
    /// Parse the contents of a JSON file specifying Configuration data
    fn parse(contents: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(contents)
    }

    /// Format a [Config] object for storage in a file
    fn format(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(self).map_err(Error::CannotFormatConfig)
    }

    /// Load configuration data from the specified file
    pub(crate) async fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let p = path.as_ref();
        debug!(?p, "Trying to load login config");
        let contents = read_to_string(path).await.map_err(|_| Error::NotLoggedIn)?;
        Self::parse(&contents).map_err(Error::ConfigParseFailed)
    }

    /// Save the current [Config] object to the given file path
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

    /// Create a new [Config] object
    pub(crate) fn new(username: String, password: String, database: PathBuf) -> Self {
        Config {
            username,
            password,
            database,
        }
    }

    /// Check whether the current [Config] object represents a valid configuration
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

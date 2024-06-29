use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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

impl Config {
    fn parse(contents: String) -> Result<Self> {
        serde_json::from_str(&contents).with_context(|| "Couldn't parse json string")
    }

    fn format(&self) -> Result<String> {
        serde_json::to_string_pretty(self).with_context(|| "Couldn't convert config to json")
    }

    pub async fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let p = path.as_ref();
        debug!(?p, "Trying to load login config");
        let contents = read_to_string(path).await?;
        Self::parse(contents)
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
}

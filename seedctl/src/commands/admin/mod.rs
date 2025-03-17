//! Commands for administration of seedctl or the database itself
use crate::cli::AdminCommands;
use anyhow::Result;
use std::path::PathBuf;

mod database;
mod germination;
mod user;

/// Handle the `seedctl admin` command and its subcommands
pub(crate) async fn handle_command(dbpath: Option<PathBuf>, command: AdminCommands) -> Result<()> {
    match command {
        AdminCommands::Users { command } => user::handle_command(dbpath, command).await,
        AdminCommands::Germination { command } => {
            germination::handle_command(dbpath, command).await
        }
        AdminCommands::Database { command } => database::handle_command(dbpath, command).await,
    }
}

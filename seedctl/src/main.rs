use crate::{
    cli::*,
    config::*,
    output::rows::{TaxonRow, TaxonRowDetails},
    table::SeedctlTable,
};
use anyhow::{anyhow, Result};
use clap::Parser;
use libseed::{
    loadable::Loadable,
    taxonomy::{filter_by, Taxon},
    Error::DatabaseRowNotFound,
};
use std::path::PathBuf;
use tabled::Table;
use tokio::fs;
use tracing::debug;

mod cli;
mod commands;
mod config;
mod output;
mod prompt;
mod table;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Cli::parse();
    let xdgdirs = xdg::BaseDirectories::new()?;
    let config_file = xdgdirs.place_config_file("seedctl/config")?;
    match &args.command {
        Commands::Login { username, database } => {
            let username = username
                .as_ref()
                .cloned()
                .or_else(|| inquire::Text::new("Username:").prompt().ok())
                .ok_or_else(|| anyhow!("No username specified"))?;
            let database = database
                .as_ref()
                .cloned()
                .or_else(|| {
                    inquire::Text::new("Database path:")
                        .prompt()
                        .map(PathBuf::from)
                        .ok()
                })
                .ok_or_else(|| anyhow!("No database specified"))?;
            let pwd = inquire::Password::new("Password:")
                .with_display_toggle_enabled()
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .without_confirmation()
                .prompt()?;
            let cfg = Config::new(username.clone(), pwd, database.clone());
            cfg.validate().await?;
            cfg.save_to_file(&config_file).await?;
            println!("Logged in as {username}");
            return Ok(());
        }
        Commands::Logout => {
            fs::remove_file(&config_file)
                .await
                .or_else(|e| match e.kind() {
                    std::io::ErrorKind::NotFound => Ok(()),
                    _ => Err(anyhow::Error::from(e)),
                })?;
            println!("Logged out");
            return Ok(());
        }
        _ => (),
    };

    let cfg = config::Config::load_from_file(&config_file).await?;
    debug!(?cfg.username, ?cfg.database, "logging in");
    let (dbpool, user) = cfg.validate().await?;

    match args.command {
        Commands::Login { .. } => {
            Ok(()) // already handled above
        }
        Commands::Logout => Ok(()),
        Commands::Status => {
            println!("Using database '{}'", cfg.database.to_string_lossy());
            println!("Logged in as user '{}'", cfg.username);
            Ok(())
        }
        Commands::Projects { command } => {
            commands::projects::handle_command(command, user, &dbpool).await
        }
        Commands::Sources { command } => {
            commands::sources::handle_command(command, user, &dbpool).await
        }
        Commands::Samples { command } => {
            commands::samples::handle_command(command, user, &dbpool).await
        }
        Commands::Taxonomy { command } => match command {
            TaxonomyCommands::Find {
                rank,
                genus,
                species,
                any,
                minnesota,
            } => {
                let minnesota = match minnesota {
                    true => Some(true),
                    false => None,
                };
                let taxa: Vec<Taxon> = Taxon::load_all(
                    filter_by(None, rank, genus, species, any, minnesota),
                    None,
                    &dbpool,
                )
                .await?;
                if taxa.is_empty() {
                    return Err(anyhow!("No results found"));
                }
                let mut table = Table::new(taxa.iter().map(TaxonRow::new));
                println!("{}\n", table.styled());
                println!("{} records found", taxa.len());
                Ok(())
            }
            TaxonomyCommands::Show { id, output } => match Taxon::load(id, &dbpool).await {
                Ok(mut taxon) => {
                    let str = output::format_one(
                        TaxonRowDetails::new(&mut taxon, &dbpool).await?,
                        output,
                    )?;
                    println!("{str}");
                    Ok(())
                }
                Err(DatabaseRowNotFound(_)) => {
                    println!("Taxon {id} not found");
                    Ok(())
                }
                Err(e) => Err(e.into()),
            },
        },
        Commands::Admin { command } => {
            commands::admin::handle_command(command, user, &dbpool).await
        }
    }
}

use crate::cli::*;
use crate::config::*;
use crate::table::SeedctlTable;
use crate::table::*;
use anyhow::{anyhow, Context, Result};
use clap::Parser;
use libseed::{
    loadable::Loadable,
    taxonomy::{filter_by, Taxon},
    user::User,
    Error::DatabaseRowNotFound,
};
use sqlx::SqlitePool;
use std::path::PathBuf;
use tabled::Table;
use tracing::debug;

mod cli;
mod commands;
mod config;
mod prompt;
mod table;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Cli::parse();
    let xdgdirs = xdg::BaseDirectories::new()?;
    let config_file = xdgdirs.place_config_file("seedctl/config")?;
    let cfg = match &args.command {
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
                        .map(|p| PathBuf::from(p))
                        .ok()
                })
                .ok_or_else(|| anyhow!("No database specified"))?;
            let pwd = inquire::Password::new("Password:")
                .with_display_toggle_enabled()
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .without_confirmation()
                .prompt()?;
            let cfg = Config::new(username.clone(), pwd, database.clone());
            cfg.save_to_file(&config_file).await.map(|_| cfg)
        }
        _ => config::Config::load_from_file(&config_file).await,
    }
    .with_context(|| "Must log in before issuing any other commands")?;

    debug!(?cfg.username, ?cfg.database, "logging in");
    let dbpool =
        SqlitePool::connect(&format!("sqlite://{}", cfg.database.to_string_lossy())).await?;
    sqlx::migrate!("../db/migrations").run(&dbpool).await?;
    let user = User::load_by_username(&cfg.username, &dbpool)
        .await
        .with_context(|| "Failed to fetch user from database".to_string())?
        .ok_or_else(|| anyhow!("Unable to find user {}", cfg.username))?;
    user.verify_password(&cfg.password)?;

    match args.command {
        Commands::Login { .. } => {
            Ok(()) // already handled above
        }
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
                let mut table = Table::new(taxa.iter().map(|t| TaxonRow::new(t)));
                println!("{}\n", table.styled());
                println!("{} records found", taxa.len());
                Ok(())
            }
            TaxonomyCommands::Show { id } => match Taxon::load(id, &dbpool).await {
                Ok(mut taxon) => {
                    let tbuilder =
                        Table::builder(vec![TaxonRowDetails::new(&mut taxon, &dbpool).await?])
                            .index()
                            .column(0)
                            .transpose();
                    println!("{}\n", tbuilder.build().styled());
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

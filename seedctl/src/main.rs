//! This is a command-line tool to manage a seed collection via [libseed]
use crate::{
    cli::*,
    config::*,
    output::{
        rows::{TaxonRow, TaxonRowDetails},
        table::SeedctlTable,
    },
};
use anyhow::{Result, anyhow};
use clap::Parser;
use libseed::{
    Error::DatabaseError,
    core::{
        loadable::Loadable,
        query::filter::{Cmp, and},
    },
    taxonomy::{self, Taxon, quickfind},
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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Cli::parse();
    let config_file = config_file().await?;
    let config_db = config::Config::load_from_file(&config_file)
        .await
        .map(|cfg| cfg.database)
        .ok();

    match args.command {
        Commands::Admin { database, command } => {
            return commands::admin::handle_command(database.or(config_db), command).await;
        }
        Commands::Login { username, database } => {
            let username = username
                .or_else(|| inquire::Text::new("Username:").prompt().ok())
                .ok_or_else(|| anyhow!("No username specified"))?;
            let database = database
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
    let (db, user) = cfg.validate().await?;

    match args.command {
        // already handled above
        Commands::Login { .. } | Commands::Logout | Commands::Admin { .. } => Ok(()),
        Commands::Status => {
            println!("Using database '{}'", cfg.database.to_string_lossy());
            println!("Logged in as user '{}'", cfg.username);
            Ok(())
        }
        Commands::Projects { command } => {
            commands::projects::handle_command(command, user, &db).await
        }
        Commands::Sources { command } => {
            commands::sources::handle_command(command, user, &db).await
        }
        Commands::Samples { command } => {
            commands::samples::handle_command(command, user, &db).await
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
                let mut filter = and();
                if let Some(id) = None {
                    filter = filter.push(taxonomy::Filter::Id(Cmp::Equal, id));
                }
                if let Some(rank) = rank {
                    filter = filter.push(taxonomy::Filter::Rank(rank));
                }
                if let Some(genus) = genus {
                    filter = filter.push(taxonomy::Filter::Genus(genus));
                }
                if let Some(species) = species {
                    filter = filter.push(taxonomy::Filter::Species(species));
                }
                if let Some(s) = any {
                    if let Some(f) = quickfind(&s) {
                        filter = filter.push(f);
                    }
                }
                if let Some(val) = minnesota {
                    filter = filter.push(taxonomy::Filter::Minnesota(val));
                }

                let taxa: Vec<Taxon> =
                    Taxon::load_all(Some(filter.build()), None, None, &db).await?;
                if taxa.is_empty() {
                    return Err(anyhow!("No results found"));
                }
                let mut table = Table::new(taxa.iter().map(TaxonRow::new));
                println!("{}\n", table.styled());
                println!("{} records found", taxa.len());
                Ok(())
            }
            TaxonomyCommands::Show { id, output } => match Taxon::load(id, &db).await {
                Ok(mut taxon) => {
                    let str = output::format_one(
                        TaxonRowDetails::new(&mut taxon, &db).await?,
                        output.format,
                    )?;
                    println!("{str}");
                    Ok(())
                }
                Err(DatabaseError(sqlx::Error::RowNotFound)) => {
                    println!("Taxon {id} not found");
                    Ok(())
                }
                Err(e) => Err(e.into()),
            },
        },
    }
}

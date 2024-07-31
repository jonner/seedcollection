use anyhow::{anyhow, Context, Result};
use clap::Parser;
use libseed::{
    loadable::Loadable,
    taxonomy::{filter_by, Germination, Taxon},
    user::{User, UserStatus},
    Error::DatabaseRowNotFound,
};
use sqlx::SqlitePool;
use std::io::{stdin, stdout, Write};
use std::path::PathBuf;
use tabled::Table;
use tokio::fs;
use tracing::debug;

use crate::cli::*;
use crate::config::*;
use crate::table::SeedctlTable;
use crate::table::*;

mod cli;
mod commands;
mod config;
mod prompt;
mod table;

async fn get_password(path: Option<PathBuf>, message: Option<String>) -> anyhow::Result<String> {
    let password = match path {
        None => {
            /* read from stdin*/
            let mut s = String::new();
            print!("{}", message.unwrap_or("New password: ".to_string()));
            stdout().flush()?;
            stdin().read_line(&mut s)?;
            s
        }
        Some(f) => fs::read_to_string(f).await?,
    };
    Ok(password.trim().to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Cli::parse();
    let xdgdirs = xdg::BaseDirectories::new()?;
    let config_file = xdgdirs.place_config_file("seedctl/config")?;
    let cfg = match &args.command {
        Commands::Login { username, database } => {
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
                println!("{} records found", table.count_rows());
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
        Commands::Admin { command } => match command {
            AdminCommands::User { command } => match command {
                UserCommands::List {} => {
                    let users = User::load_all(&dbpool).await?;
                    let mut table = Table::new(users.iter().map(|u| UserRow::new(u)));
                    println!("{}\n", table.styled());
                    println!("{} records found", table.count_rows());
                    Ok(())
                }
                UserCommands::Add {
                    username,
                    email,
                    passwordfile,
                } => {
                    let password = get_password(
                        passwordfile,
                        Some(format!("New password for '{username}': ")),
                    )
                    .await?;
                    // hash the password
                    let pwhash = User::hash_password(&password)?;
                    let mut user = User::new(
                        username.clone(),
                        email.clone(),
                        pwhash,
                        UserStatus::Unverified,
                        None,
                        None,
                        None,
                    );
                    let id = user.insert(&dbpool).await?.last_insert_rowid();
                    println!("Added user to database:");
                    println!("{}: {}", id, username);
                    Ok(())
                }
                UserCommands::Remove { id } => User::delete_id(&id, &dbpool)
                    .await
                    .map(|_| ())
                    .with_context(|| "failed to remove user"),
                UserCommands::Modify {
                    id,
                    username,
                    change_password,
                    password_file,
                } => {
                    let mut user = User::load(id, &dbpool).await?;
                    if let Some(username) = username {
                        user.username = username;
                    }
                    if change_password {
                        let password = get_password(password_file, None).await?;
                        user.change_password(&password)?;
                    }
                    user.update(&dbpool)
                        .await
                        .map(|_| ())
                        .with_context(|| "Failed to modify user")
                }
            },
            AdminCommands::Germination { command } => match command {
                GerminationCommands::List {} => {
                    let codes = Germination::load_all(&dbpool).await?;
                    let mut table = Table::new(codes.iter().map(|g| GerminationRow::new(g)));
                    println!("{}\n", table.styled());
                    Ok(())
                }
                GerminationCommands::Modify {
                    id,
                    interactive,
                    code,
                    summary,
                    description,
                } => {
                    let oldval = Germination::load(id, &dbpool).await?;
                    let mut newval = oldval.clone();
                    if interactive {
                        println!("Modifying Germination code {id}. Pres <esc to skip any field.");
                        println!("Current code: '{}'", oldval.code);
                        if let Some(code) = inquire::Text::new("Code:").prompt_skippable()? {
                            newval.code = code;
                        }
                        println!(
                            "Current summary: '{}'",
                            oldval
                                .summary
                                .as_ref()
                                .cloned()
                                .unwrap_or_else(|| "<null>".to_string())
                        );
                        if let Some(summary) = inquire::Text::new("Summary:").prompt_skippable()? {
                            newval.summary = Some(summary);
                        }
                        println!(
                            "Current description: '{}'",
                            oldval
                                .description
                                .as_ref()
                                .cloned()
                                .unwrap_or_else(|| "<null>".to_string())
                        );
                        if let Some(description) = inquire::Editor::new("Description:")
                            .with_predefined_text(
                                oldval
                                    .description
                                    .as_ref()
                                    .map(|v| v.as_str())
                                    .unwrap_or_else(|| ""),
                            )
                            .prompt_skippable()?
                        {
                            newval.description = Some(description);
                        }
                    } else {
                        if code.is_none() && summary.is_none() && description.is_none() {
                            return Err(anyhow!(
                                "No fields specified. Cannot modify germination code."
                            ));
                        }
                        if let Some(code) = code {
                            newval.code = code;
                        }
                        if let Some(summary) = summary {
                            newval.summary = Some(summary);
                        }
                        if let Some(description) = description {
                            newval.description = Some(description);
                        }
                    }
                    if oldval != newval {
                        debug!("Submitting new value for germination code: {:?}", newval);
                        newval.update(&dbpool).await?;
                        println!("Modified germination code...");
                    }
                    Ok(())
                }
            },
        },
    }
}

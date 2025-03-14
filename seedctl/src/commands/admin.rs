//! Commands for administration of seedctl or the database itself
use crate::{
    Error,
    cli::{AdminCommands, GerminationCommands, UserCommands},
    output::{
        self,
        rows::{GerminationRow, UserRow},
    },
};
use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use libseed::{
    database::{Database, UpgradeAction},
    loadable::Loadable,
    taxonomy::Germination,
    user::{User, UserStatus},
};
use std::io::BufReader;
use std::io::Write;
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

/// Get a password from the user, either from the file at the provided path, or
/// by reading input from stdin.
async fn get_password(path: Option<PathBuf>) -> anyhow::Result<String> {
    let password = match path {
        None => {
            let query = inquire::Password::new("New Password:")
                .with_display_toggle_enabled()
                .with_display_mode(inquire::PasswordDisplayMode::Masked);
            query.prompt()?
        }
        Some(f) => fs::read_to_string(f).await?,
    };
    Ok(password.trim().to_string())
}

/// Handle the `seedctl admin` command and its subcommands
pub(crate) async fn handle_command(dbpath: Option<PathBuf>, command: AdminCommands) -> Result<()> {
    match command {
        AdminCommands::Users { command } => {
            let db = Database::open(
                dbpath
                    .ok_or_else(|| Error::InvalidArgument("No database specified".to_string()))?,
            )
            .await?;
            match command {
                UserCommands::List { output } => {
                    let users = User::load_all(&db).await?;
                    let str = output::format_seq(users.iter().map(UserRow::new), output.format)?;
                    println!("{str}");
                    Ok(())
                }
                UserCommands::Add {
                    username,
                    email,
                    passwordfile,
                } => {
                    let username = username
                        .or_else(|| inquire::Text::new("Username:").prompt().ok())
                        .ok_or_else(|| anyhow!("No username specified"))?;
                    let email = email
                        .or_else(|| inquire::Text::new("Email Address:").prompt().ok())
                        .ok_or_else(|| anyhow!("No email address specified"))?;
                    let password = get_password(passwordfile).await?;
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
                    let id = user.insert(&db).await?.last_insert_rowid();
                    println!("Added user to database:");
                    println!("{}: {}", id, username);
                    Ok(())
                }
                UserCommands::Remove { id } => {
                    match inquire::Confirm::new("Really remove user?")
                        .with_default(false)
                        .prompt()?
                    {
                        true => User::delete_id(&id, &db)
                            .await
                            .map(|_| ())
                            .with_context(|| "failed to remove user"),
                        false => Ok(()),
                    }
                }
                UserCommands::Modify {
                    id,
                    username,
                    change_password,
                    passwordfile,
                } => {
                    let mut user = User::load(id, &db).await?;
                    if let Some(username) = username {
                        user.username = username;
                    }
                    if change_password {
                        let password = get_password(passwordfile).await?;
                        user.change_password(&password)?;
                    }
                    user.update(&db)
                        .await
                        .map(|_| ())
                        .with_context(|| "Failed to modify user")
                }
            }
        }
        AdminCommands::Germination { command } => {
            let db = Database::open(
                dbpath
                    .ok_or_else(|| Error::InvalidArgument("No database specified".to_string()))?,
            )
            .await?;
            match command {
                GerminationCommands::List { output } => {
                    let codes = Germination::load_all(&db).await?;
                    let str =
                        output::format_seq(codes.iter().map(GerminationRow::new), output.format)?;
                    println!("{str}");
                    Ok(())
                }
                GerminationCommands::Modify {
                    id,
                    code,
                    summary,
                    description,
                } => {
                    let oldval = Germination::load(id, &db).await?;
                    let mut newval = oldval.clone();
                    if code.is_none() && summary.is_none() && description.is_none() {
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
                            .with_predefined_text(oldval.description.as_deref().unwrap_or_default())
                            .prompt_skippable()?
                        {
                            newval.description = Some(description);
                        }
                    } else {
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
                        newval.update(&db).await?;
                        println!("Modified germination code...");
                    }
                    Ok(())
                }
            }
        }
        AdminCommands::Database { command } => match command {
            crate::DatabaseCommands::Upgrade {
                new_database,
                zipfile,
                download,
            } => {
                let db =
                    Database::open(dbpath.ok_or_else(|| {
                        Error::InvalidArgument("No database specified".to_string())
                    })?)
                    .await?;
                let newdbfile = match new_database {
                    Some(path) => {
                        println!("Using new database at {path:?}");
                        path
                    }
                    None => {
                        let zipfile = match zipfile {
                            Some(zipfile) => {
                                println!("Using zipfile {zipfile:?}");
                                std::fs::File::open(zipfile)?
                            }
                            None => {
                                if !download {
                                    return Err(anyhow!("No new database specified"));
                                }
                                download_latest_itis().await?
                            }
                        };
                        itis_extract_database(zipfile)?
                    }
                };
                db.upgrade(newdbfile, |summary| {
                    for taxon_change in summary.changes.iter() {
                        println!("Taxon '{}' changed:", taxon_change.taxon.complete_name);
                        for mismatch in taxon_change.changes.iter() {
                            println!(
                                " - Field '{}' changed from '{}' to '{}'",
                                mismatch.property_name, mismatch.old_value, mismatch.new_value
                            )
                        }
                    }
                    for replacement in summary.replacements.iter() {
                        println!(
                            "Taxon '{}' ({}) will be changed to '{}' ({})",
                            replacement.old.complete_name,
                            replacement.old.id,
                            replacement.new.complete_name,
                            replacement.new.id
                        )
                    }
                    match inquire::Confirm::new("Proceed with database upgrade?")
                        .with_default(false)
                        .prompt()
                    {
                        Ok(true) => UpgradeAction::Proceed,
                        _ => UpgradeAction::Abort,
                    }
                })
                .await?;
                Ok(())
            }
        },
    }
}

fn itis_extract_database(archivefile: std::fs::File) -> Result<PathBuf> {
    let mut archive = zip::ZipArchive::new(archivefile)?;
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.name().ends_with("ITIS.sqlite") {
            debug!("Found sqlite database '{}'", file.name());
            let path = PathBuf::from("ITIS.sqlite");
            let mut dbfile = std::fs::File::create(&path)?;
            let mut stream = BufReader::new(file);
            std::io::copy(&mut stream, &mut dbfile)?;
            debug!("extracted database from zip file");
            return Ok(path);
        }
    }
    Err(anyhow!("Unable to find sqlite database within zip file"))
}

async fn download_latest_itis() -> Result<std::fs::File> {
    let mut latest_file = tempfile::tempfile()?;
    let itis_url = "https://www.itis.gov/downloads/itisSqlite.zip";
    println!("Downloading latest database from '{itis_url}'");
    let mut stream = reqwest::get(itis_url).await?.bytes_stream();
    // FIXME: provide some progress monitoring
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        debug!(".");
        latest_file.write_all(&chunk)?;
    }
    latest_file.flush()?;
    println!("Downloaded file");
    Ok(latest_file)
}

use std::{
    io::{stdin, stdout, Write},
    path::PathBuf,
};

use crate::{
    cli::{AdminCommands, GerminationCommands, UserCommands},
    table::{GerminationRow, SeedctlTable, UserRow},
};
use anyhow::{anyhow, Context, Result};
use libseed::{
    loadable::Loadable,
    taxonomy::Germination,
    user::{User, UserStatus},
};
use sqlx::{Pool, Sqlite};
use tabled::Table;
use tokio::fs;
use tracing::debug;

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

pub async fn handle_command(
    command: AdminCommands,
    _user: User,
    dbpool: &Pool<Sqlite>,
) -> Result<()> {
    match command {
        AdminCommands::Users { command } => match command {
            UserCommands::List {} => {
                let users = User::load_all(dbpool).await?;
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
                let id = user.insert(dbpool).await?.last_insert_rowid();
                println!("Added user to database:");
                println!("{}: {}", id, username);
                Ok(())
            }
            UserCommands::Remove { id } => User::delete_id(&id, dbpool)
                .await
                .map(|_| ())
                .with_context(|| "failed to remove user"),
            UserCommands::Modify {
                id,
                username,
                change_password,
                password_file,
            } => {
                let mut user = User::load(id, dbpool).await?;
                if let Some(username) = username {
                    user.username = username;
                }
                if change_password {
                    let password = get_password(password_file, None).await?;
                    user.change_password(&password)?;
                }
                user.update(dbpool)
                    .await
                    .map(|_| ())
                    .with_context(|| "Failed to modify user")
            }
        },
        AdminCommands::Germination { command } => match command {
            GerminationCommands::List {} => {
                let codes = Germination::load_all(dbpool).await?;
                let mut table = Table::new(codes.iter().map(|g| GerminationRow::new(g)));
                println!("{}\n", table.styled());
                Ok(())
            }
            GerminationCommands::Modify {
                id,
                code,
                summary,
                description,
            } => {
                let oldval = Germination::load(id, dbpool).await?;
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
                    newval.update(dbpool).await?;
                    println!("Modified germination code...");
                }
                Ok(())
            }
        },
    }
}

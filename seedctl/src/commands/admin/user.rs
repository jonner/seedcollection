use crate::{
    cli::UserCommands,
    output::{self, rows::UserRow},
};

use anyhow::{Context, Result, anyhow};
use libseed::{
    Database,
    core::loadable::Loadable,
    user::{User, UserStatus},
};
use tokio::fs;

use std::path::PathBuf;

/// Get a password from the user, either from the file at the provided path, or
/// by reading input from stdin.
async fn get_password(path: Option<PathBuf>) -> Result<String> {
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

pub(crate) async fn handle_command(dbpath: Option<PathBuf>, command: UserCommands) -> Result<()> {
    let db = Database::open(dbpath.ok_or_else(|| anyhow!("No database specified"))?).await?;
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

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

pub(crate) async fn handle_command(dbpath: Option<PathBuf>, command: UserCommands) -> Result<()> {
    let db = Database::open(dbpath.ok_or_else(|| anyhow!("No database specified"))?).await?;
    match command {
        UserCommands::List { output } => list_users(&db, output).await,
        UserCommands::Add {
            username,
            email,
            passwordfile,
        } => add_user(&db, username, email, passwordfile).await,
        UserCommands::Remove { id } => remove_user(&db, id).await,
        UserCommands::Modify {
            id,
            username,
            change_password,
            passwordfile,
        } => modify_user(&db, id, username, change_password, passwordfile).await,
    }
}

async fn modify_user(
    db: &Database,
    id: i64,
    username: Option<String>,
    change_password: bool,
    passwordfile: Option<PathBuf>,
) -> Result<()> {
    let mut user = User::load(id, db).await?;
    if let Some(username) = username {
        user.username = username;
    }
    if change_password {
        let password = get_password(passwordfile).await?;
        user.change_password(&password)?;
    }
    user.update(db)
        .await
        .map(|_| ())
        .with_context(|| "Failed to modify user")
}

async fn remove_user(db: &Database, id: i64) -> Result<()> {
    match inquire::Confirm::new("Really remove user?")
        .with_default(false)
        .prompt()?
    {
        true => User::delete_id(&id, db)
            .await
            .map(|_| ())
            .with_context(|| "failed to remove user"),
        false => Ok(()),
    }
}

async fn add_user(
    db: &Database,
    username: Option<String>,
    email: Option<String>,
    passwordfile: Option<PathBuf>,
) -> Result<()> {
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
    user.insert(db).await?;
    println!("Added user to database:");
    println!("{}: {}", user.id, username);
    Ok(())
}

async fn list_users(db: &Database, output: crate::OutputOptions) -> Result<()> {
    let users = User::load_all(db).await?;
    let str = output::format_seq(users.iter().map(UserRow::new), output.format)?;
    println!("{str}");
    Ok(())
}

/// Utility function to get a password from the user, either from the file at
/// the provided path, or by reading input from stdin.
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

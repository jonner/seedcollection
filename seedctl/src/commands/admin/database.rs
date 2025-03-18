use crate::{
    cli::DatabaseCommands,
    config::{self, Config},
};

use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use indicatif::{HumanBytes, ProgressBar};
use libseed::{Database, core::database::UpgradeAction};
use tracing::debug;

use std::{
    io::{BufReader, Write},
    path::PathBuf,
    time::Duration,
};

pub(crate) async fn handle_command(
    dbpath: Option<PathBuf>,
    command: DatabaseCommands,
) -> Result<()> {
    match command {
        DatabaseCommands::Init {
            new_database,
            zipfile,
            download,
            admin_user,
            admin_email,
            passwordfile,
        } => {
            initialize_database(
                dbpath,
                new_database,
                zipfile,
                download,
                admin_user,
                admin_email,
                passwordfile,
            )
            .await
        }
        DatabaseCommands::Upgrade {
            new_database,
            zipfile,
            download,
        } => upgrade_database(dbpath, new_database, zipfile, download).await,
    }
}

async fn upgrade_database(
    dbpath: Option<PathBuf>,
    new_database: Option<PathBuf>,
    zipfile: Option<PathBuf>,
    download: bool,
) -> std::result::Result<(), anyhow::Error> {
    let dbpath = dbpath.ok_or_else(|| anyhow!("No database specified"))?;
    let db = Database::open(&dbpath).await?;
    let response = inquire::Confirm::new(&format!("Upgrading database '{}'. Make sure that your database is backed up before proceeding. Continue?", dbpath.display()))
        .with_default(false)
        .prompt()?;
    if !response {
        return Err(inquire::InquireError::OperationCanceled.into());
    }
    let newdbfile = resolve_database_file(new_database, zipfile, download).await?;
    db.upgrade(newdbfile, |summary| {
        if !summary.is_empty() {
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
        } else {
            println!("No relevant changes detected when upgrading to the new database.");
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

async fn initialize_database(
    dbpath: Option<PathBuf>,
    new_database: Option<PathBuf>,
    zipfile: Option<PathBuf>,
    download: bool,
    admin_user: Option<String>,
    admin_email: Option<String>,
    passwordfile: Option<PathBuf>,
) -> std::result::Result<(), anyhow::Error> {
    let project_dirs = directories::ProjectDirs::from("org", "quotidian", "seedcollection")
        .ok_or_else(|| anyhow!("Cannot find default project data directory"))?;
    let mut default_db_path = project_dirs.data_dir().to_path_buf();
    default_db_path.push("seedcollection.sqlite");
    let dest_path = dbpath.unwrap_or(default_db_path);
    println!(
        "Attempting to Initialize new seedcollection database at '{}'...",
        dest_path.display()
    );
    if tokio::fs::try_exists(&dest_path).await?
        && !(inquire::Confirm::new(&format!(
            "Overwrite existing database file '{}'",
            dest_path.display(),
        ))
        .prompt()?)
    {
        return Err(anyhow!("Refusing to overwrite existing database file"));
    }
    let source_db = resolve_database_file(new_database, zipfile, download).await?;
    let db_parent_dir = dest_path
        .parent()
        .ok_or_else(|| anyhow!("Couldn't determine path for database"))?;
    tokio::fs::create_dir_all(db_parent_dir).await?;
    debug!("Copying {source_db:?} to {dest_path:?}");
    tokio::fs::copy(source_db, &dest_path).await?;
    let mut db = Database::open(&dest_path).await?;
    let username = admin_user
        .or_else(|| inquire::Text::new("Administrator username:").prompt().ok())
        .ok_or_else(|| anyhow!("No Administrator username specified"))?;
    let email = admin_email
        .or_else(|| {
            inquire::Text::new("Administrator email address:")
                .prompt()
                .ok()
        })
        .ok_or_else(|| anyhow!("No Administrator email specified"))?;
    let password = match passwordfile {
        Some(f) => tokio::fs::read_to_string(f).await?,
        None => inquire::Password::new("Administrator password:")
            .with_display_toggle_enabled()
            .with_display_mode(inquire::PasswordDisplayMode::Masked)
            .prompt()?,
    };
    let user = db.init(username.clone(), email, password.clone()).await?;
    println!("Added user to database:");
    println!("{}: {}", user.id, user.username);
    let cfg = Config::new(username.clone(), password, dest_path);
    cfg.validate().await?;
    cfg.save_to_file(&config::config_file().await?).await?;
    println!("Logged in as {username}");
    Ok(())
}

fn itis_extract_database(archivefile: std::fs::File) -> Result<PathBuf> {
    let mut archive = zip::ZipArchive::new(archivefile)
        .with_context(|| "Failed to open zip archive {archivefile:?}")?;
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.name().ends_with("ITIS.sqlite") {
            debug!("Found sqlite database '{}'", file.name());
            let (mut dbfile, path) = tempfile::NamedTempFile::new()?.keep()?;
            let mut stream = BufReader::new(file);
            std::io::copy(&mut stream, &mut dbfile)
                .with_context(|| "Failed to copy temp file to {path:?}")?;
            debug!("extracted database from zip file into {:?}", path);
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
    let progress = ProgressBar::new_spinner();
    progress.enable_steady_tick(Duration::from_millis(100));
    let mut total_downloaded: u64 = 0;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        total_downloaded += chunk.len() as u64;
        progress.set_message(format!("{}", HumanBytes(total_downloaded)));
        latest_file.write_all(&chunk)?;
    }
    latest_file.flush()?;
    progress.finish();
    println!("Downloaded file");
    Ok(latest_file)
}

async fn resolve_database_file(
    new_database: Option<PathBuf>,
    zipfile: Option<PathBuf>,
    download: bool,
) -> Result<PathBuf, anyhow::Error> {
    let itisdbfile = match new_database {
        Some(path) => {
            println!("Using new taxonomy database at '{}'", path.display());
            path
        }
        None => {
            let zipfile = match zipfile {
                Some(zipfile) => {
                    println!(
                        "Using new taxonomy database from compressed file '{}'",
                        zipfile.display()
                    );
                    std::fs::File::open(zipfile)?
                }
                None => {
                    if !download {
                        return Err(anyhow!("No new taxonomy database specified"));
                    }
                    download_latest_itis().await?
                }
            };
            itis_extract_database(zipfile)?
        }
    };
    Ok(itisdbfile)
}

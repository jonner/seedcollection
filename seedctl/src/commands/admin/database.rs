use crate::{
    cli::DatabaseCommands,
    config::{self, Config},
};
use anyhow::{Context, Result, anyhow};
use csv::ReaderBuilder;
use futures::StreamExt;
use indicatif::{HumanBytes, ProgressBar, ProgressIterator};
use libseed::{
    Database,
    core::database::UpgradeAction,
    taxonomy::{Germination, KINGDOM_PLANTAE, NativeStatus, Rank, Taxon},
};
use sqlx::Row;
use tracing::{debug, trace, warn};

use std::{
    collections::HashMap,
    fs::File,
    io::{BufReader, Write},
    path::{Path, PathBuf},
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
        DatabaseCommands::UpdateNativeStatus { args } => {
            handle_native_list(dbpath, args.specieslist, args.updatedb, args.show_options).await
        }
        DatabaseCommands::UpdateGerminationInfo { args } => {
            handle_germination_list(dbpath, args.specieslist, args.updatedb, args.show_options)
                .await
        }
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

const NATIVE_STATUS_FIELDS: [&str; 9] = [
    "X",
    "genus",
    "X",
    "species",
    "subttype",
    "subtaxa",
    "native_status",
    "rarity_status",
    "invasive_status",
];

const GERMINATION_FIELDS: [&str; 7] = [
    "X", "genus", "X", "species", "subttype", "subtaxa", "germcode",
];

// --- Structs for Database Rows (derive sqlx::FromRow) ---
#[derive(Debug, sqlx::FromRow)]
struct TsnResult {
    tsn: i32,
}

async fn find_genus_synonym(db: &Database, genus: &str) -> anyhow::Result<Option<String>> {
    debug!("Looking for a synonym for {}", genus);

    let tsn_record: Option<TsnResult> = sqlx::query_as(
        r#"SELECT S.tsn_accepted as tsn from taxonomic_units T
           INNER JOIN synonym_links S ON T.tsn=S.tsn
           WHERE name_usage='not accepted' AND unit_name1=?1
           AND kingdom_id=?2 and rank_id=?3"#,
    )
    .bind(genus)
    .bind(KINGDOM_PLANTAE)
    .bind(Rank::Genus as i32)
    .fetch_optional(db.pool())
    .await?;

    if let Some(tsn_res) = tsn_record {
        debug!("Found synonym {}, looking up info about it", tsn_res.tsn);

        // Corrected to select only 'genus' as per Python logic (which only returns row["genus"])
        let accepted_genus_info = sqlx::query(
            r#"SELECT T.unit_name1 as genus FROM taxonomic_units T
               WHERE T.tsn=?1 AND name_usage='accepted' AND kingdom_id=?2"#,
        )
        .bind(tsn_res.tsn)
        .bind(KINGDOM_PLANTAE)
        .fetch_optional(db.pool())
        .await?;

        if let Some(ag_info) = accepted_genus_info {
            let genus: String = ag_info.try_get("genus")?;
            trace!("Accepted Genus Info: genus={genus}");
            return Ok(Some(genus)); // ag_info.genus is already Option<String>
        }
    }
    Ok(None)
}

fn displayname(name1: &str, name2: &str, name3: &str) -> String {
    let mut parts = Vec::new();
    if !name1.is_empty() {
        parts.push(name1);
    }
    if !name2.is_empty() {
        parts.push(name2);
    }
    if !name3.is_empty() {
        parts.push(name3);
    }
    parts.join(" ")
}

async fn find_synonym(
    db: &Database,
    name1: &str,
    name2: &str,
    name3: &str,
    rank: Rank,
) -> sqlx::Result<Option<Taxon>> {
    let dname = displayname(name1, name2, name3);
    debug!("Looking for a synonym for {}", dname);

    let accepted_tsn_row: Option<TsnResult> = if rank == Rank::Species {
        sqlx::query_as(
            r#"SELECT S.tsn_accepted as tsn from taxonomic_units T
               INNER JOIN synonym_links S ON T.tsn=S.tsn
               WHERE name_usage='not accepted' AND unit_name1=?1 and unit_name2=?2
               AND kingdom_id=?3 and rank_id=?4"#,
        )
        .bind(name1)
        .bind(name2)
        .bind(KINGDOM_PLANTAE)
        .bind(rank as i32)
        .fetch_optional(db.pool())
        .await?
    } else {
        sqlx::query_as(
            r#"SELECT S.tsn_accepted as tsn from taxonomic_units T
               INNER JOIN synonym_links S ON T.tsn=S.tsn
               WHERE name_usage='not accepted' AND unit_name1=?1 and unit_name2=?2
               AND unit_name3=?3 AND kingdom_id=?4 and rank_id=?5"#,
        )
        .bind(name1)
        .bind(name2)
        .bind(name3)
        .bind(KINGDOM_PLANTAE)
        .bind(rank as i32)
        .fetch_optional(db.pool())
        .await?
    };

    if let Some(row) = accepted_tsn_row {
        trace!("Found synonym TSN for {}: {}", dname, row.tsn);
        debug!("Found synonym {}, looking up info about it", row.tsn);
        sqlx::query_as(
            r#"SELECT T.*,
               GROUP_CONCAT(V.vernacular_name) AS common_names
               FROM taxonomic_units T LEFT JOIN vernaculars V ON V.tsn=T.tsn
               WHERE T.tsn=?1 AND name_usage='accepted' AND kingdom_id=?2
               GROUP BY T.tsn"#,
        )
        .bind(row.tsn)
        .bind(KINGDOM_PLANTAE)
        .fetch_optional(db.pool())
        .await
    } else {
        Ok(None)
    }
}

#[derive(Debug, sqlx::FromRow)]
struct PossibilityRow {
    tsn: i64,
    complete_name: String,
}

async fn find_possibilities(
    db: &Database,
    name1: &str,
    name2: &str,
    name3: &str,
    rank: Rank,
) -> sqlx::Result<Vec<PossibilityRow>> {
    debug!("Looking for other possibilities for {} {}", name1, name2);

    if rank == Rank::Species {
        sqlx::query_as("SELECT tsn, complete_name FROM taxonomic_units T WHERE unit_name1 like ? OR unit_name2 LIKE ? AND kingdom_id=?")
            .bind(name1)
            .bind(name2)
            .bind(KINGDOM_PLANTAE)
            .fetch_all(db.pool())
            .await
    } else {
        sqlx::query_as("SELECT tsn, complete_name FROM taxonomic_units T WHERE unit_name1 like ? OR unit_name2 OR unit_name3 LIKE ? AND kingdom_id=?")
            .bind(name1)
            .bind(name2)
            .bind(name3)
            .bind(KINGDOM_PLANTAE)
            .fetch_all(db.pool())
            .await
    }
}

async fn get_taxon(
    db: &Database,
    name1: &str,
    name2: &str,
    name3: &str,
    rank: Rank,
) -> sqlx::Result<Option<(Taxon, bool)>> {
    let mut is_synonym = false;
    debug!(
        "Looking up information for ({}, {}, {}, {})",
        name1, name2, name3, rank
    );

    let taxon: Option<Taxon> = if rank == Rank::Species {
        sqlx::query_as(
            r#"SELECT T.*,
               GROUP_CONCAT(V.vernacular_name) as common_names
               FROM taxonomic_units T LEFT JOIN vernaculars V ON V.tsn=T.tsn
               WHERE T.unit_name1=?1 AND T.unit_name2=?2 AND T.name_usage='accepted'
               AND T.kingdom_id=?3 AND T.rank_id=?4
               GROUP BY T.tsn"#,
        )
        .bind(name1)
        .bind(name2)
        .bind(KINGDOM_PLANTAE)
        .bind(rank as i32)
        .fetch_optional(db.pool())
        .await?
    } else {
        sqlx::query_as(
            r#"SELECT T.*,
               GROUP_CONCAT(V.vernacular_name) as common_names
               FROM taxonomic_units T LEFT JOIN vernaculars V ON V.tsn=T.tsn
               WHERE T.unit_name1=?1 AND T.unit_name2=?2 AND T.unit_name3=?3
               AND T.name_usage='accepted' AND T.kingdom_id=?4 AND T.rank_id=?5
               GROUP BY T.tsn"#,
        )
        .bind(name1)
        .bind(name2)
        .bind(name3)
        .bind(KINGDOM_PLANTAE)
        .bind(rank as i32)
        .fetch_optional(db.pool())
        .await?
    };

    let taxon = match taxon {
        Some(taxon) => Some(taxon),
        None => find_synonym(db, name1, name2, name3, rank)
            .await
            .inspect(|_| is_synonym = true)?,
    };

    Ok(taxon.map(|info| (info, is_synonym)))
}

fn combine_status(old_status: NativeStatus, new_status: NativeStatus) -> NativeStatus {
    if old_status == NativeStatus::Unknown {
        return new_status;
    } else if new_status == NativeStatus::Unknown {
        return old_status;
    }
    if old_status == NativeStatus::Native || new_status == NativeStatus::Native {
        return NativeStatus::Native;
    }
    NativeStatus::Introduced
}

async fn handle_taxa_list(
    pool: &Database,
    reader: &mut csv::Reader<File>,
    show_options: bool,
    fields: &[&str],
) -> anyhow::Result<Vec<(Taxon, csv::StringRecord)>> {
    let mut taxa: Vec<(Taxon, csv::StringRecord)> = Vec::new();
    let headers = reader.headers()?.clone();
    let mut n_not_found = 0;
    trace!("Handling taxa list");

    // verify headers are as expected
    let fieldnames: &csv::StringRecord = &headers;
    if fieldnames.len() != fields.len() {
        return Err(anyhow!(
            "Expected {} fields, found {}",
            fields.len(),
            fieldnames.len()
        ));
    }
    for (i, expected_field) in fields.iter().enumerate() {
        if let Some(actual_field) = fieldnames.get(i) {
            if *expected_field != actual_field {
                return Err(anyhow!(
                    "Field name mismatch. Expected '{}' in col {}, found '{}'",
                    expected_field,
                    i,
                    actual_field
                ));
            }
        } else {
            return Err(anyhow!(
                "Missing field in col {} (expected '{}')",
                i,
                expected_field
            ));
        }
    }

    // determine number of records
    let first_record = reader.position().clone();
    let records = reader.records();
    let nrecords: u64 = records.count().try_into()?;
    trace!(nrecords);

    reader.seek(first_record)?;
    println!("Analyzing species list and matching against database...");
    let progress = ProgressBar::new(nrecords);
    for result in reader.records() {
        progress.inc(1);
        let record = result?;
        let get_field = |col: usize| -> String { record.get(col).unwrap_or("").trim().to_string() };

        let ind1 = get_field(0);
        let name1 = get_field(1);
        let ind2 = get_field(2);
        let name2 = get_field(3);
        let ind3 = get_field(4);
        let name3 = get_field(5);

        if name1.is_empty() && name2.is_empty() {
            warn!("Skipping row with empty genus and species.");
            continue;
        }
        let dname = displayname(&name1, &name2, &name3);
        if ind1 == "X" || ind2 == "X" {
            debug!("Skipping hybrid {} x {} for now", name1, name2);
            continue;
        }

        let mut rank = Rank::Species;
        if ind3 == "var." {
            rank = Rank::Variety;
        } else if ind3 == "subsp." {
            rank = Rank::Subspecies;
        }

        if let Some((taxon, is_synonym)) = get_taxon(pool, &name1, &name2, &name3, rank).await? {
            debug!("Found taxon {dname}: {}", taxon.complete_name);
            if is_synonym {
                progress.println(format!(
                    "Using '{}' as a synonym for '{dname}'",
                    taxon.complete_name
                ))
            }
            taxa.push((taxon, record));
            continue;
        }

        if let Some(new_genus) = find_genus_synonym(pool, &name1).await? {
            if let Some((taxon, _)) = get_taxon(pool, &new_genus, &name2, &name3, rank).await? {
                progress.println(format!(
                    "Using '{}' as a synonym for '{dname}'",
                    taxon.complete_name
                ));
                taxa.push((taxon, record));
                continue;
            }
        }

        n_not_found += 1;
        progress.println(format!(
            "WARNING: Unable to find an exact match for {dname}. ",
        ));
        if show_options {
            let rows = find_possibilities(pool, &name1, &name2, &name3, rank).await?;

            if !rows.is_empty() {
                progress.println(format!(
                    "Unable to find an exact match for '{dname}'. Possibilities:"
                ));
                for row in &rows {
                    progress.println(format!("  - {}: {}", row.tsn, row.complete_name));
                }
            }
        }
    }
    progress.finish_and_clear();

    if !show_options && n_not_found > 0 {
        println!(
            "{n_not_found} taxa were unable to be found in the database. Please run with '--show-options' to show a list of taxa similar to the missing taxon."
        )
    }
    Ok(taxa)
}

async fn handle_germination_list(
    dbpath: Option<PathBuf>,
    specieslist: PathBuf,
    updatedb: bool,
    show_options: bool,
) -> anyhow::Result<()> {
    let (db, matched_taxa) = common_setup(
        dbpath.unwrap_or_else(|| "seedcollection.sqlite".into()),
        specieslist,
        show_options,
        &GERMINATION_FIELDS,
    )
    .await?;

    let germinations = Germination::load_all(&db).await?;
    let germination_map: HashMap<String, i64> = germinations
        .into_iter()
        .map(|germ| (germ.code, germ.id))
        .collect();
    trace!(?germination_map);
    let germ_codes = matched_taxa
        .into_iter()
        .map(|(taxon, csvrecord)| {
            let code = csvrecord.get(6).ok_or_else(|| {
                anyhow!(
                    "CSV file doesn't have germination code field for {}",
                    taxon.complete_name
                )
            })?;
            let germid = germination_map.get(code).ok_or_else(|| {
                anyhow!("Failed to find database id for germination code '{code}'")
            })?;
            trace!(
                "Found germination id {germid} for taxon {}",
                taxon.complete_name
            );
            Ok((taxon, germid))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    if !germ_codes.is_empty() {
        if updatedb
            || inquire::Confirm::new(&format!(
                "Matched {} taxa. Update database?",
                germ_codes.len()
            ))
            .with_default(false)
            .prompt()?
        {
            println!("Adding {} items to the database...", germ_codes.len());
            let mut tx = db.pool().begin().await?; // Start transaction

            for (taxon, germid) in germ_codes.into_iter().progress() {
                // it's possible for multiple taxa in the input list to map
                // to a single taxon in the database, so we may get constraint
                // violations here if they both map to the same taxon id and
                // germination id, thus the "OR IGNORE".
                sqlx::query(
                    "INSERT OR IGNORE INTO sc_taxon_germination (tsn, germid) VALUES (?1, ?2)",
                )
                .bind(taxon.id)
                .bind(germid)
                .execute(&mut *tx) // Use the transaction
                .await?;
            }
            tx.commit().await?; // Commit transaction
            println!("Database update complete.");
        } else {
            println!("Database not updated.");
        }
    } else {
        println!("No taxa data to update in the database.");
    }

    Ok(())
}

async fn handle_native_list(
    dbpath: Option<PathBuf>,
    specieslist: PathBuf,
    updatedb: bool,
    show_options: bool,
) -> anyhow::Result<()> {
    let (db, matched_taxa) = common_setup(
        dbpath.unwrap_or_else(|| "seedcollection.sqlite".into()),
        specieslist,
        show_options,
        &NATIVE_STATUS_FIELDS,
    )
    .await?;

    let taxa_map = matched_taxa
        .into_iter()
        .fold(HashMap::new(), |mut acc, val| {
            if let Some(new_status) = val
                .1
                .get(6)
                .and_then(|val| val.parse::<NativeStatus>().ok())
            {
                acc.entry(val.0)
                    .and_modify(|old_status| *old_status = combine_status(*old_status, new_status))
                    .or_insert(new_status);
            }
            acc
        });
    if !taxa_map.is_empty() {
        if updatedb
            || inquire::Confirm::new(&format!(
                "Matched {} taxa. Update database?",
                taxa_map.len()
            ))
            .with_default(false)
            .prompt()?
        {
            println!("Adding {} items to the database...", taxa_map.len());
            let mut tx = db.pool().begin().await?; // Start transaction

            sqlx::query("DELETE FROM 'mntaxa'")
                .execute(&mut *tx)
                .await?;
            debug!("Deleted all records from mntaxa");

            for (taxon, native_status) in taxa_map.iter().progress() {
                sqlx::query("INSERT INTO mntaxa (tsn, native_status) VALUES (?1, ?2)")
                    .bind(taxon.id)
                    .bind(native_status.to_string())
                    .execute(&mut *tx) // Use the transaction
                    .await?;
            }
            tx.commit().await?; // Commit transaction
            println!("Database update complete.");
        } else {
            println!("Database not updated.",);
        }
    } else {
        println!("No taxa data to update in the database.");
    }

    Ok(())
}

async fn common_setup(
    db_path: PathBuf,
    specieslist: PathBuf,
    show_options: bool,
    fields: &[&str],
) -> Result<(Database, Vec<(Taxon, csv::StringRecord)>), anyhow::Error> {
    if !Path::new(&specieslist).exists() {
        return Err(anyhow!(
            "Species list CSV file not found: {:?}",
            specieslist
        ));
    }
    let db = Database::open(&db_path).await?;
    trace!("Connected to database: {:?}", db_path);
    let csv_file = File::open(specieslist)?;
    let mut csvreader = ReaderBuilder::new().has_headers(true).from_reader(csv_file);
    let matched_taxa = handle_taxa_list(&db, &mut csvreader, show_options, fields).await?;
    Ok((db, matched_taxa))
}

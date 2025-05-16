use anyhow::anyhow;
use clap::{Parser, Subcommand};
use csv::ReaderBuilder;
use indicatif::ProgressIterator;
use sqlx::{Row, SqlitePool};
use std::{collections::HashMap, fmt::Display, fs::File, path::Path, str::FromStr};
use tracing::{debug, info, warn};

const KINGDOM_PLANTAE: i32 = 3;
const RANK_GENUS: i32 = 180;
const RANK_SPECIES: i32 = 220;
const RANK_SUBSPECIES: i32 = 230;
const RANK_VARIETY: i32 = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeStatus {
    Native,
    Introduced,
    Unknown,
}

impl FromStr for NativeStatus {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "N" => Ok(NativeStatus::Native),
            "U" => Ok(NativeStatus::Unknown),
            "I" => Ok(NativeStatus::Introduced),
            _ => Err(anyhow!("Unknown native status {s}")),
        }
    }
}

impl Display for NativeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = match self {
            NativeStatus::Native => 'N',
            NativeStatus::Introduced => 'I',
            NativeStatus::Unknown => 'U',
        };
        write!(f, "{c}")
    }
}
const CSV_FIELDS: [&str; 9] = [
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

// --- Structs for Database Rows (derive sqlx::FromRow) ---
#[derive(Debug, sqlx::FromRow)]
struct TsnResult {
    tsn: i32,
}

#[derive(Debug, sqlx::FromRow)]
struct TaxonInfo {
    tsn: i32,
    complete_name: String,
    common_names: Option<String>, // GROUP_CONCAT can result in NULL
}

async fn find_genus_synonym(pool: &SqlitePool, genus: &str) -> anyhow::Result<Option<String>> {
    info!("Looking for a synonym for {}", genus);

    let tsn_record: Option<TsnResult> = sqlx::query_as(
        r#"SELECT S.tsn_accepted as tsn from taxonomic_units T
           INNER JOIN synonym_links S ON T.tsn=S.tsn
           WHERE name_usage='not accepted' AND unit_name1=?1
           AND kingdom_id=?2 and rank_id=?3"#,
    )
    .bind(genus)
    .bind(KINGDOM_PLANTAE)
    .bind(RANK_GENUS)
    .fetch_optional(pool)
    .await?;

    if let Some(tsn_res) = tsn_record {
        debug!("Found synonym TSN: {}", tsn_res.tsn);
        info!("Found synonym {}, looking up info about it", tsn_res.tsn);

        // Corrected to select only 'genus' as per Python logic (which only returns row["genus"])
        let accepted_genus_info = sqlx::query(
            r#"SELECT T.unit_name1 as genus FROM taxonomic_units T
               WHERE T.tsn=?1 AND name_usage='accepted' AND kingdom_id=?2"#,
        )
        .bind(tsn_res.tsn)
        .bind(KINGDOM_PLANTAE)
        .fetch_optional(pool)
        .await?;

        if let Some(ag_info) = accepted_genus_info {
            let genus: String = ag_info.try_get("genus")?;
            debug!("Accepted Genus Info: genus={genus}");
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
    pool: &SqlitePool,
    name1: &str,
    name2: &str,
    name3: &str,
    rank: i32,
) -> sqlx::Result<Option<TaxonInfo>> {
    let dname = displayname(name1, name2, name3);
    info!("Looking for a synonym for {}", dname);

    let tsn_record_opt: Option<TsnResult> = if rank == RANK_SPECIES {
        sqlx::query_as(
            r#"SELECT S.tsn_accepted as tsn from taxonomic_units T
               INNER JOIN synonym_links S ON T.tsn=S.tsn
               WHERE name_usage='not accepted' AND unit_name1=?1 and unit_name2=?2
               AND kingdom_id=?3 and rank_id=?4"#,
        )
        .bind(name1)
        .bind(name2)
        .bind(KINGDOM_PLANTAE)
        .bind(rank)
        .fetch_optional(pool)
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
        .bind(rank)
        .fetch_optional(pool)
        .await?
    };

    if let Some(tsn_res) = tsn_record_opt {
        debug!("Found synonym TSN for {}: {}", dname, tsn_res.tsn);
        info!("Found synonym {}, looking up info about it", tsn_res.tsn);
        sqlx::query_as(
            r#"SELECT T.tsn, T.complete_name,
               GROUP_CONCAT(V.vernacular_name) AS common_names, T.rank_id
               FROM taxonomic_units T LEFT JOIN vernaculars V ON V.tsn=T.tsn
               WHERE T.tsn=?1 AND name_usage='accepted' AND kingdom_id=?2
               GROUP BY T.tsn"#,
        )
        .bind(tsn_res.tsn)
        .bind(KINGDOM_PLANTAE)
        .fetch_optional(pool)
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
    pool: &SqlitePool,
    name1: &str,
    name2: &str,
    name3: &str,
    rank: i32,
) -> sqlx::Result<Vec<PossibilityRow>> {
    info!("Looking for other possibilities for {} {}", name1, name2);

    if rank == RANK_SPECIES {
        sqlx::query_as("SELECT tsn, complete_name FROM taxonomic_units T WHERE unit_name1 like ? OR unit_name2 LIKE ? AND kingdom_id=?")
            .bind(name1)
            .bind(name2)
            .bind(KINGDOM_PLANTAE)
            .fetch_all(pool)
            .await
    } else {
        sqlx::query_as("SELECT tsn, complete_name FROM taxonomic_units T WHERE unit_name1 like ? OR unit_name2 OR unit_name3 LIKE ? AND kingdom_id=?")
            .bind(name1)
            .bind(name2)
            .bind(name3)
            .bind(KINGDOM_PLANTAE)
            .fetch_all(pool)
            .await
    }
}

async fn get_taxon(
    pool: &SqlitePool,
    name1: &str,
    name2: &str,
    name3: &str,
    rank: i32,
) -> sqlx::Result<Option<i32>> {
    let mut synonym_found_flag = false;
    let dname = displayname(name1, name2, name3);
    info!(
        "Looking up information for ({}, {}, {}, {})",
        name1, name2, name3, rank
    );

    let taxon_info_opt: Option<TaxonInfo> = if rank == RANK_SPECIES {
        sqlx::query_as(
            r#"SELECT T.tsn, T.complete_name,
               GROUP_CONCAT(V.vernacular_name) as common_names
               FROM taxonomic_units T LEFT JOIN vernaculars V ON V.tsn=T.tsn
               WHERE T.unit_name1=?1 AND T.unit_name2=?2 AND T.name_usage='accepted'
               AND T.kingdom_id=?3 AND T.rank_id=?4
               GROUP BY T.tsn"#,
        )
        .bind(name1)
        .bind(name2)
        .bind(KINGDOM_PLANTAE)
        .bind(rank)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as(
            r#"SELECT T.tsn, T.complete_name,
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
        .bind(rank)
        .fetch_optional(pool)
        .await?
    };

    let final_taxon_info = match taxon_info_opt {
        Some(info) => Some(info),
        None => {
            synonym_found_flag = true;
            find_synonym(pool, name1, name2, name3, rank).await?
        }
    };

    if let Some(info) = final_taxon_info {
        let cname = info
            .common_names
            .as_deref()
            .unwrap_or("no common name known");
        let prefix = if synonym_found_flag { "*" } else { "" };
        info!(
            "{}{} is <{}> {} ({})",
            prefix, dname, info.tsn, info.complete_name, cname
        );
        Ok(Some(info.tsn))
    } else {
        Ok(None)
    }
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
    pool: &SqlitePool,
    reader: &mut csv::Reader<File>,
    print_options: bool,
    fields: &[&str],
) -> anyhow::Result<Vec<(i32, csv::StringRecord)>> {
    let mut taxa: Vec<(i32, csv::StringRecord)> = Vec::new();
    let headers = reader.headers()?.clone();
    debug!("Handling taxa list");

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
    debug!(nrecords);

    reader.seek(first_record)?;
    println!("Analyzing species list and matching against database...");
    for result in reader.records().progress_count(nrecords) {
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
            info!("Skipping hybrid {} x {} for now", name1, name2);
            continue;
        }

        let mut rank = RANK_SPECIES;
        if ind3 == "var." {
            rank = RANK_VARIETY;
        } else if ind3 == "subsp." {
            rank = RANK_SUBSPECIES;
        }

        if let Some(tsn) = get_taxon(pool, &name1, &name2, &name3, rank).await? {
            taxa.push((tsn, record));
            continue;
        }

        if let Some(new_genus) = find_genus_synonym(pool, &name1).await? {
            info!(
                "Genus {} is a synonym for {}, using new name {} {}",
                name1, new_genus, new_genus, name2
            );
            if let Some(tsn) = get_taxon(pool, &new_genus, &name2, &name3, rank).await? {
                taxa.push((tsn, record));
                continue;
            }
        }

        if print_options {
            let rows = find_possibilities(pool, &name1, &name2, &name3, rank).await?;

            if rows.is_empty() {
                warn!("Unable to find species '{}'", dname)
            } else {
                debug!("Possibilities for '{}':", dname);
                for row in &rows {
                    debug!(dname, ?row);
                    println!("  - {}: {}", row.tsn, row.complete_name);
                }
            }
        } else {
            warn!(
                "Unable to find an exact match for {}. Pass --show-options to view possible matches",
                dname
            )
        }
    }
    Ok(taxa)
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, default_value = "ITIS.sqlite")]
    db: String,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    MatchSpecies {
        specieslist: String,
        #[clap(long)]
        updatedb: bool,
        #[clap(long)]
        show_options: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    let args = Args::parse();

    match args.command {
        Commands::MatchSpecies {
            specieslist,
            updatedb,
            show_options,
        } => match_species(&args.db, &specieslist, updatedb, show_options).await,
    }
}

async fn match_species(
    db: &str,
    specieslist: &str,
    updatedb: bool,
    show_options: bool,
) -> anyhow::Result<()> {
    let db_url = format!("sqlite://{}?mode=rwc", db); // Ensure create if not exists for `updatedb`
    if !Path::new(db).exists() && !updatedb {
        // only error if not updating and not exists
        return Err(anyhow!("Database file not found: {}", db));
    }
    if !Path::new(&specieslist).exists() {
        return Err(anyhow!("Species list CSV file not found: {}", specieslist));
    }

    let pool = SqlitePool::connect(&db_url).await?;
    debug!("Connected to database: {}", db);

    let csv_file = File::open(specieslist)?;
    let mut csvreader = ReaderBuilder::new().has_headers(true).from_reader(csv_file);

    let matched_taxa = handle_taxa_list(&pool, &mut csvreader, show_options, &CSV_FIELDS).await?;

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
        if updatedb {
            println!("Adding {} items to the database...", taxa_map.len());
            let mut tx = pool.begin().await?; // Start transaction

            sqlx::query("DELETE FROM 'mntaxa'")
                .execute(&mut *tx)
                .await?;
            info!("Deleted all records from mntaxa");

            for (tsn, native_status) in taxa_map.iter().progress() {
                sqlx::query("INSERT INTO mntaxa (tsn, native_status) VALUES (?1, ?2)")
                    .bind(tsn)
                    .bind(native_status.to_string())
                    .execute(&mut *tx) // Use the transaction
                    .await?;
            }
            tx.commit().await?; // Commit transaction
            println!("Database update complete.");
        } else {
            println!(
                "Database update not requested. Matched {} taxa. Run with `--updatedb` to update the database.",
                taxa_map.len()
            );
        }
    } else {
        println!("No taxa data to update in the database.");
    }

    pool.close().await;
    Ok(())
}

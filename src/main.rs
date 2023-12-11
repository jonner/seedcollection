use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use sqlx::{Connection, Row, SqliteConnection};
use std::path::PathBuf;
use std::str::FromStr;
use tokio;
use log::debug;

pub fn logger() -> env_logger::Builder {
    let env = env_logger::Env::new()
        .filter_or("SC_LOG", "warn")
        .write_style("SC_LOG_STYLE");
    env_logger::Builder::from_env(env)
}

#[derive(Debug, Clone)]
enum TaxonRank {
    Kingdom = 10,
    Division = 30,
    Class = 60,
    Order = 100,
    Family = 140,
    Genus = 180,
    Species = 220,
    Subspecies = 230,
    Variety = 240,
}

impl FromStr for TaxonRank {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> anyhow::Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "kingdom" => Ok(TaxonRank::Kingdom),
            "division" => Ok(TaxonRank::Division),
            "class" => Ok(TaxonRank::Class),
            "order" => Ok(TaxonRank::Order),
            "family" => Ok(TaxonRank::Family),
            "genus" => Ok(TaxonRank::Genus),
            "species" => Ok(TaxonRank::Species),
            "subspecies" => Ok(TaxonRank::Subspecies),
            "variety" => Ok(TaxonRank::Variety),
            _ => Err(anyhow!("invalid rank {}", s)),
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[arg(short, long, default_value = "seedcollection.sqlite")]
    database: PathBuf,
    #[arg(short, long)]
    native: bool,
    #[arg(long, action = clap::ArgAction::Count)]
    debug: u8,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Checklist,
    Collection {
        #[command(subcommand)]
        command: CollectionCommands,
    },
    Location {
        #[command(subcommand)]
        command: LocationCommands,
    },
    Sample {
        #[command(subcommand)]
        command: SampleCommands,
    },
    Taxon {
        #[command(subcommand)]
        command: TaxonCommands,
    },
}

#[derive(Subcommand, Debug)]
enum CollectionCommands {
    List,
    Add {
        name: String,
        description: Option<String>,
    },
    Modify {
        id: u32,
        name: Option<String>,
        description: Option<String>,
    },
    Remove {
        id: u32,
    },
}

#[derive(Subcommand, Debug)]
enum LocationCommands {
    List {
        #[arg(short, long)]
        full: bool,
    },
    Add {
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "lat")]
        latitude: Option<f32>,
        #[arg(long = "long")]
        longitude: Option<f32>,
    },
    Remove {
        id: u32,
    },
    Modify,
}

#[derive(Subcommand, Debug)]
enum SampleCommands {
    List,
    Add,
    Remove,
    Modify,
}

#[derive(Subcommand, Debug)]
enum TaxonCommands {
    Find {
        #[arg(long)]
        id: Option<i64>,
        #[arg(long)]
        rank: Option<TaxonRank>,
        #[arg(long)]
        genus: Option<String>,
        #[arg(long)]
        species: Option<String>,
        #[arg(long)]
        any: Option<String>,
    },
}

fn print_taxon(
    tsn: i64,
    complete_name: &Option<String>,
    common_names: &Option<String>,
    isnative: bool,
    _full: bool,
) {
    println!(
        "{}: {}{}{}",
        tsn,
        complete_name.as_ref().unwrap_or(&"".to_string()),
        common_names
            .as_ref()
            .map(|x| if x.is_empty() {
                "".to_string()
            } else {
                format!(" ({x})")
            })
            .unwrap_or("".to_string()),
        match isnative {
            true => "",
            false => "*",
        }
    );
}

fn build_taxon_query(
    tsn: Option<i64>,
    rank: Option<TaxonRank>,
    genus: Option<String>,
    species: Option<String>,
    any: Option<String>,
) -> sqlx::QueryBuilder<'static, sqlx::Sqlite> {
    let mut builder: sqlx::QueryBuilder<sqlx::Sqlite> = sqlx::QueryBuilder::new(
        r#"SELECT T.tsn, T.complete_name, M.native_status, GROUP_CONCAT(V.vernacular_name, ", ") as cnames
                                      FROM taxonomic_units T
                                      INNER JOIN mntaxa M on T.tsn=M.tsn
                                      INNER JOIN hierarchy H on H.tsn=T.tsn
                                      LEFT JOIN vernaculars V on V.tsn=T.tsn"#,
    );

    let mut firstclause = true;
    if let Some(id) = tsn {
        builder.push(" WHERE T.tsn=");
        builder.push_bind(id);
        firstclause = false;
    }
    if let Some(rank) = rank {
        if !firstclause {
            builder.push(" AND");
        } else {
            builder.push(" WHERE");
        }
        builder.push(" rank_id=");
        builder.push_bind(rank as i64);
        firstclause = false;
    }
    if let Some(genus) = genus {
        if !firstclause {
            builder.push(" AND");
        } else {
            builder.push(" WHERE");
        }
        builder.push(" unit_name1 LIKE ");
        builder.push_bind(genus);
        firstclause = false;
    }
    if let Some(species) = species {
        if !firstclause {
            builder.push(" AND");
        } else {
            builder.push(" WHERE");
        }
        builder.push(" unit_name2 LIKE ");
        builder.push_bind(species);
        firstclause = false;
    }

    if let Some(any) = any {
        if !firstclause {
            builder.push(" AND");
        } else {
            builder.push(" WHERE");
        }
        let any = format!("%{any}%");
        builder.push(" (");
        let fields = ["unit_name1", "unit_name2", "unit_name3", "V.vernacular_name"];
        let mut first = true;
        for field in fields {
            if !first {
                builder.push(" OR");
            }
            first = false;
            builder.push(format!(" {field} LIKE "));
            builder.push_bind(any.clone());
            builder.push(")");
        }
        builder.push(" )");
    }

    builder.push(" GROUP BY T.tsn ORDER BY hierarchy_string");
    debug!("generated sql: <<{}>>", builder.sql());
    builder
}

#[tokio::main]
async fn main() -> Result<()> {
    logger().init();
    let args = Cli::parse();
    let mut dbconn =
        SqliteConnection::connect(&format!("sqlite://{}", args.database.to_string_lossy())).await?;
    match args.command {
        Some(Commands::Checklist) => {
            let results = build_taxon_query(None, None, None, None, None).build().fetch_all(&mut dbconn).await?;
            for row in results {
                let isnative : Option<String> = row.get("native_status");
                let isnative = isnative == Some("N".to_string());
                if args.native && !isnative {
                    continue;
                }
                print_taxon(row.get("tsn"), &row.get("complete_name"), &row.get("cnames"), isnative, false);
            }
            Ok(())
        }
        Some(Commands::Collection { command }) => match command {
            CollectionCommands::List => {
                println!("No collections found");
                Ok(())
            }
            _ => Err(anyhow!("{command:?} subcommand not implemented yet")),
        },
        Some(Commands::Location { command }) => match command {
            LocationCommands::List { full } => {
                let results = sqlx::query!(
                    r#"SELECT L.locid, L.name, L.description
                                      FROM seedlocations L"#,
                )
                .fetch_all(&mut dbconn)
                .await?;
                println!("{} records found\n", results.len());
                for row in results {
                    if full {
                        println!(
                            "{}: {}{}",
                            row.locid,
                            row.name.unwrap_or("".to_string()),
                            row.description
                                .map(|x| format!(" - {x}"))
                                .unwrap_or("".to_string())
                        );
                    } else {
                        println!("{}: {}", row.locid, row.name.unwrap_or("".to_string()),);
                    }
                }
                Ok(())
            }
            LocationCommands::Add {
                name,
                description,
                latitude,
                longitude,
            } => {
                sqlx::query!(
                    r#"INSERT INTO seedlocations (name, description, latitude, longitude)
                VALUES (?1, ?2, ?3, ?4)"#,
                    name,
                    description,
                    latitude,
                    longitude
                )
                .execute(&mut dbconn)
                .await?
                .rows_affected();
                Ok(())
            }
            LocationCommands::Remove { id } => {
                sqlx::query!(r#"DELETE FROM seedlocations WHERE locid=?1"#, id)
                    .execute(&mut dbconn)
                    .await?;
                Ok(())
            }
            _ => Err(anyhow!("{command:?} subcommand not implemented yet")),
        },
        Some(Commands::Sample { command }) => match command {
            SampleCommands::List => {
                println!("No samples found");
                Ok(())
            }
            _ => Err(anyhow!("{command:?} subcommand not implemented yet")),
        },
        Some(Commands::Taxon { command }) => match command {
            TaxonCommands::Find {
                id,
                rank,
                genus,
                species,
                any,
            } => {
                let results;
                let mut query = build_taxon_query(id, rank, genus, species, any);
                results = query.build().fetch_all(&mut dbconn).await?;
                if results.is_empty() {
                    return Err(anyhow!("No results found"));
                }
                for row in results {
                    let isnative : Option<String> = row.get("native_status");
                    let isnative = isnative == Some("N".to_string());
                    print_taxon(row.get("tsn"), &row.get("complete_name"), &row.get("cnames"), isnative, false);
                }
                Ok(())
            }
        },
        None => Err(anyhow!("Missing command")),
    }
}

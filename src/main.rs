use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use log::debug;
use sqlx::{Row, SqlitePool};
use std::path::PathBuf;
use std::str::FromStr;
use strum_macros::{Display, EnumString, FromRepr};
use tokio;

pub fn logger() -> env_logger::Builder {
    let env = env_logger::Env::new()
        .filter_or("SC_LOG", "warn")
        .write_style("SC_LOG_STYLE");
    env_logger::Builder::from_env(env)
}

const KINGDOM_PLANTAE: i64 = 3;
#[derive(Debug, Clone, Display, EnumString, FromRepr)]
#[strum(ascii_case_insensitive)]
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

#[derive(Debug, Display, EnumString, FromRepr)]
enum NativeStatus {
    #[strum(serialize = "Native", serialize = "N")]
    Native,
    #[strum(serialize = "Introduced", serialize = "I")]
    Introduced,
    #[strum(serialize = "Unknown", serialize = "U")]
    Unknown,
}

fn print_table(builder: tabled::builder::Builder, nrecs: usize) {
    use tabled::settings::Style; //, Modify, object::Segment, width::Width, object::Columns};
    println!("{}\n", builder.build().with(Style::psql())); //.with(Modify::new(Columns::single(2)).with(Width::wrap(50))));
    println!("{} records found", nrecs);
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Cli {
    #[arg(short, long, default_value = "seedcollection.sqlite")]
    database: PathBuf,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Query taxonomy")]
    Taxonomy {
        #[command(subcommand)]
        command: TaxonomyCommands,
    },
    #[command(about = "Manage seed collections")]
    Collection {
        #[command(subcommand)]
        command: CollectionCommands,
    },
    #[command(about = "Manage locations")]
    Location {
        #[command(subcommand)]
        command: LocationCommands,
    },
    #[command(about = "Manage samples")]
    Sample {
        #[command(subcommand)]
        command: SampleCommands,
    },
}

#[derive(Subcommand, Debug)]
enum CollectionCommands {
    #[command(about = "List all collections")]
    List {
        #[arg(short, long)]
        full: bool,
    },
    #[command(about = "Add a new collection to the database")]
    Add {
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        description: Option<String>,
    },
    #[command(
        about="Modify properties of a collection",
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["name", "description"]),
        ))]
    Modify {
        #[arg(short, long)]
        id: i64,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long)]
        description: Option<String>,
    },
    #[command(about = "Remove a collection from the database")]
    Remove { id: i64 },
    #[command(about = "Add a new sample to the collection")]
    AddSample {
        #[arg(short, long)]
        collection: i64,
        #[arg(short, long)]
        sample: i64,
    },
    #[command(about = "Remove an existing sample from the collection")]
    RemoveSample {
        #[arg(short, long)]
        collection: i64,
        #[arg(short, long)]
        sample: i64,
    },
    #[command(about = "Show all details about a collection")]
    Show { id: i64 },
}

#[derive(Subcommand, Debug)]
enum LocationCommands {
    #[command(about = "List all locations")]
    List {
        #[arg(short, long)]
        full: bool,
    },
    #[command(about = "Add a new location to the database")]
    Add {
        #[arg(long)]
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "lat")]
        latitude: Option<f64>,
        #[arg(long = "long")]
        longitude: Option<f64>,
    },
    #[command(about = "Remove an existing location from the database")]
    Remove { id: i64 },
    #[command(
        about="Modify properties about a location",
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["name", "description", "latitude", "longitude"]),
        ))]
    Modify {
        #[arg(long)]
        id: i64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "lat")]
        latitude: Option<f64>,
        #[arg(long = "long")]
        longitude: Option<f64>,
    },
}

#[derive(Subcommand, Debug)]
enum SampleCommands {
    #[command(about = "List all samples")]
    List,
    #[command(about = "Add a new sample to the database")]
    Add {
        #[arg(long)]
        taxon_id: i64,
        #[arg(long)]
        location_id: i64,
    },
    #[command(about = "Remove an existing sample from the database")]
    Remove { id: i64 },
    #[command(
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["taxon_id", "location_id"]),
        ),
        about="Modify properties of a sample")]
    Modify {
        #[arg(long)]
        id: i64,
        #[arg(long)]
        taxon_id: Option<i64>,
        #[arg(long)]
        location_id: Option<i64>,
    },
}

#[derive(Subcommand, Debug)]
enum TaxonomyCommands {
    #[command(about = "Find a taxon in the database")]
    Find {
        #[arg(long, help = "Only show taxa with the given ID")]
        id: Option<i64>,
        #[arg(long, help = "Only show taxa with the given rank (e.g. 'family')")]
        rank: Option<TaxonRank>,
        #[arg(long, help = "Only show taxa in the given genus")]
        genus: Option<String>,
        #[arg(long, help = "Only show taxa in the given species")]
        species: Option<String>,
        #[arg(long, help = "Show taxa with the given string in any field")]
        any: Option<String>,
        #[arg(long, help = "Show only taxa found in Minnesota")]
        minnesota: bool,
    },
}

fn build_taxon_query(
    tsn: Option<i64>,
    rank: Option<TaxonRank>,
    genus: Option<String>,
    species: Option<String>,
    any: Option<String>,
    minnesota: bool,
) -> sqlx::QueryBuilder<'static, sqlx::Sqlite> {
    let mut builder: sqlx::QueryBuilder<sqlx::Sqlite> = sqlx::QueryBuilder::new(
        r#"SELECT T.tsn, T.complete_name, T.rank_id, M.native_status,
            GROUP_CONCAT(V.vernacular_name, ", ") as cnames
            FROM taxonomic_units T
            INNER JOIN hierarchy H on H.tsn=T.tsn
            LEFT JOIN (SELECT * FROM vernaculars WHERE
                       (language="English" or language="unspecified")) V on V.tsn=T.tsn"#,
    );

    if minnesota {
        builder.push(" INNER JOIN mntaxa M on T.tsn=M.tsn ");
    } else {
        builder.push(" LEFT JOIN mntaxa M on T.tsn=M.tsn ");
    }

    builder.push(" WHERE kingdom_id=");
    builder.push_bind(KINGDOM_PLANTAE);
    if let Some(id) = tsn {
        builder.push(" AND T.tsn=");
        builder.push_bind(id);
    }
    if let Some(rank) = rank {
        builder.push(" AND rank_id=");
        builder.push_bind(rank as i64);
    }
    if let Some(genus) = genus {
        builder.push(" AND unit_name1 LIKE ");
        builder.push_bind(genus);
    }
    if let Some(species) = species {
        builder.push(" AND unit_name2 LIKE ");
        builder.push_bind(species);
    }

    if let Some(any) = any {
        builder.push(" AND (");
        let any = format!("%{any}%");
        let fields = [
            "unit_name1",
            "unit_name2",
            "unit_name3",
            "V.vernacular_name",
        ];
        let mut first = true;
        for field in fields {
            if !first {
                builder.push(" OR");
            }
            first = false;
            builder.push(format!(" {field} LIKE "));
            builder.push_bind(any.clone());
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
    let dbpool =
        SqlitePool::connect(&format!("sqlite://{}", args.database.to_string_lossy())).await?;
    match args.command {
        Some(Commands::Collection { command }) => match command {
            CollectionCommands::List { full } => {
                let results = sqlx::query!(
                    r#"SELECT L.id, L.name, L.description
                                      FROM seedcollections L"#,
                )
                .fetch_all(&dbpool)
                .await?;
                let mut tbuilder = tabled::builder::Builder::new();
                let mut header = vec!["ID", "Name"];
                if full {
                    header.push("Description");
                }
                tbuilder.set_header(header);
                for row in &results {
                    let mut vals = vec![row.id.to_string(), row.name.clone()];
                    if full {
                        vals.push(row.description.clone().unwrap_or("".to_string()));
                    }
                    tbuilder.push_record(vals);
                }
                print_table(tbuilder, results.len());
                Ok(())
            }
            CollectionCommands::Add { name, description } => {
                let id = sqlx::query!(
                    r#"INSERT INTO seedcollections (name, description)
                VALUES (?1, ?2)"#,
                    name,
                    description,
                )
                .execute(&dbpool)
                .await?
                .last_insert_rowid();
                let row = sqlx::query!(
                    r#"SELECT L.id, L.name, L.description
                                      FROM seedcollections L WHERE id=?"#,
                    id
                )
                .fetch_one(&dbpool)
                .await?;
                println!("Added collection to database:");
                println!("{}: {}", row.id, row.name);
                Ok(())
            }
            CollectionCommands::Modify {
                id,
                name,
                description,
            } => {
                if name.is_none() && description.is_none() {
                    return Err(anyhow!("Cannot modify without new values"));
                }
                let mut builder = sqlx::QueryBuilder::new("UPDATE seedcollections SET ");
                let mut sep = builder.separated(", ");
                if let Some(name) = name {
                    sep.push("name = ");
                    sep.push_bind_unseparated(name);
                }
                if let Some(description) = description {
                    sep.push("description = ");
                    sep.push_bind_unseparated(description);
                }
                builder.push(" WHERE id=");
                builder.push_bind(id);
                debug!("sql: <<{}>>", builder.sql());
                let _res = builder.build().execute(&dbpool).await?;
                println!("Modified collection...");
                Ok(())
            }
            CollectionCommands::Remove { id } => {
                sqlx::query!(r#"DELETE FROM seedcollections WHERE id=?"#, id)
                    .execute(&dbpool)
                    .await?;
                println!("Removed collection {id}");
                Ok(())
            }
            CollectionCommands::AddSample { collection, sample } => {
                sqlx::query!(
                    r#"INSERT INTO seedcollectionsamples (collectionid, sampleid) 
                    VALUES (?, ?)"#,
                    collection,
                    sample,
                )
                .execute(&dbpool)
                .await?;
                println!("Added sample to collection");
                Ok(())
            }
            CollectionCommands::RemoveSample { collection, sample } => {
                sqlx::query!(
                    r#"DELETE FROM seedcollectionsamples WHERE collectionid=? AND sampleid=?"#,
                    collection,
                    sample,
                )
                .execute(&dbpool)
                .await?;
                println!("Removed sample from collection");
                Ok(())
            }
            CollectionCommands::Show { id } => {
                let res = sqlx::query!(
                    r#"SELECT CS.id as id, C.id as collectionid, C.name as collectionname,
                    C.description as collectiondescription, GROUP_CONCAT(V.vernacular_name) as cnames,
                    S.id as sampleid, T.complete_name FROM seedcollectionsamples CS
                    INNER JOIN seedcollections C on C.id=CS.collectionid
                    INNER JOIN seedsamples S ON S.id=CS.sampleid
                    INNER JOIN taxonomic_units T on S.tsn=T.tsn
                    LEFT JOIN (SELECT * FROM vernaculars WHERE 
                    (language="English" or language="unspecified")) V on V.tsn=T.tsn
                    WHERE collectionid=?
                    GROUP BY T.tsn"#,
                    id,
                )
                .fetch_all(&dbpool)
                .await?;
                let row = &res[0];
                println!("Collection ID: {}", row.collectionid);
                println!("Collection name: {}", row.collectionname);
                if let Some(desc) = &row.collectiondescription {
                    println!("  {}", desc);
                }
                println!("");
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.set_header(["ID", "Name", "Common Names"]);
                for row in &res {
                    tbuilder.push_record([
                        row.sampleid.to_string(),
                        row.complete_name.clone().unwrap_or("".to_string()),
                        row.cnames.clone().unwrap_or("".to_string()),
                    ]);
                }
                print_table(tbuilder, res.len());
                Ok(())
            }
        },
        Some(Commands::Location { command }) => match command {
            LocationCommands::List { full } => {
                let results = sqlx::query("SELECT * FROM seedlocations")
                    .fetch_all(&dbpool)
                    .await?;
                let mut tbuilder = tabled::builder::Builder::new();
                let mut header = vec!["ID", "Name"];
                if full {
                    header.push("Description");
                    header.push("latitude");
                    header.push("longitude");
                };
                tbuilder.set_header(header);
                for row in &results {
                    let mut vals = vec![
                        row.get::<i64, _>("locid").to_string(),
                        row.get::<String, _>("name"),
                    ];
                    if full {
                        vals.push(
                            row.get::<Option<String>, _>("description")
                                .unwrap_or("".to_string()),
                        );
                        vals.push(
                            row.get::<Option<f64>, _>("latitude")
                                .map(|n| n.to_string())
                                .unwrap_or("".to_string()),
                        );
                        vals.push(
                            row.get::<Option<f64>, _>("longitude")
                                .map(|n| n.to_string())
                                .unwrap_or("".to_string()),
                        );
                    }
                    tbuilder.push_record(vals);
                }
                print_table(tbuilder, results.len());
                Ok(())
            }
            LocationCommands::Add {
                name,
                description,
                latitude,
                longitude,
            } => {
                let newid = sqlx::query!(
                    r#"INSERT INTO seedlocations (name, description, latitude, longitude)
                VALUES (?1, ?2, ?3, ?4)"#,
                    name,
                    description,
                    latitude,
                    longitude
                )
                .execute(&dbpool)
                .await?
                .last_insert_rowid();
                println!("Added location {newid} to database");
                Ok(())
            }
            LocationCommands::Remove { id } => {
                sqlx::query!(r#"DELETE FROM seedlocations WHERE locid=?1"#, id)
                    .execute(&dbpool)
                    .await?;
                Ok(())
            }
            LocationCommands::Modify {
                id,
                name,
                description,
                latitude,
                longitude,
            } => {
                if name.is_none()
                    && description.is_none()
                    && latitude.is_none()
                    && longitude.is_none()
                {
                    return Err(anyhow!("Cannot modify location without new values"));
                }
                let mut builder = sqlx::QueryBuilder::new("UPDATE seedlocations SET ");
                let mut sep = builder.separated(", ");
                if let Some(name) = name {
                    sep.push("name = ");
                    sep.push_bind_unseparated(name);
                }
                if let Some(description) = description {
                    sep.push("description = ");
                    sep.push_bind_unseparated(description);
                }
                if let Some(latitude) = latitude {
                    sep.push("latitude = ");
                    sep.push_bind_unseparated(latitude);
                }
                if let Some(longitude) = longitude {
                    sep.push("longitude = ");
                    sep.push_bind_unseparated(longitude);
                }
                builder.push(" WHERE locid=");
                builder.push_bind(id);
                debug!("sql: <<{}>>", builder.sql());
                let _res = builder.build().execute(&dbpool).await?;
                println!("Modified collection...");
                Ok(())
            }
        },
        Some(Commands::Sample { command }) => match command {
            SampleCommands::List => {
                let results = sqlx::query!(
                    r#"SELECT S.id, S.tsn, S.collectedlocation, L.name as location_name, T.complete_name as taxon
                                      FROM seedsamples S
                                      INNER JOIN taxonomic_units T ON T.tsn=S.tsn
                                      INNER JOIN seedlocations L on L.locid=S.collectedlocation"#,
                )
                .fetch_all(&dbpool)
                .await?;
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.set_header(["ID", "Taxon", "Location"]);
                for row in &results {
                    tbuilder.push_record([
                        row.id.to_string(),
                        row.taxon.clone().unwrap(),
                        row.location_name.clone(),
                    ]);
                }
                print_table(tbuilder, results.len());
                Ok(())
            }
            SampleCommands::Add {
                taxon_id,
                location_id,
            } => {
                let newid = sqlx::query!(
                    r#"INSERT INTO seedsamples (tsn, collectedlocation)
                VALUES (?1, ?2)"#,
                    taxon_id,
                    location_id,
                )
                .execute(&dbpool)
                .await?
                .last_insert_rowid();
                println!("Added sample {newid} to database");
                Ok(())
            }
            SampleCommands::Remove { id } => {
                sqlx::query!("DELETE FROM seedsamples WHERE id=?", id,)
                    .execute(&dbpool)
                    .await?
                    .rows_affected();
                Ok(())
            }
            SampleCommands::Modify {
                id,
                taxon_id,
                location_id,
            } => {
                if taxon_id.is_none() && taxon_id.is_none() {
                    return Err(anyhow!("Cannot modify without new values"));
                }
                let mut builder = sqlx::QueryBuilder::new("UPDATE seedsamples SET ");
                let mut sep = builder.separated(", ");
                if let Some(taxon_id) = taxon_id {
                    sep.push("tsn = ");
                    sep.push_bind_unseparated(taxon_id);
                }
                if let Some(location_id) = location_id {
                    sep.push("collectedlocation = ");
                    sep.push_bind_unseparated(location_id);
                }
                builder.push(" WHERE id=");
                builder.push_bind(id);
                debug!("sql: <<{}>>", builder.sql());
                let _res = builder.build().execute(&dbpool).await?;
                println!("Modified collection...");
                Ok(())
            }
        },
        Some(Commands::Taxonomy { command }) => match command {
            TaxonomyCommands::Find {
                id,
                rank,
                genus,
                species,
                any,
                minnesota,
            } => {
                let results;
                let mut query = build_taxon_query(id, rank, genus, species, any, minnesota);
                results = query.build().fetch_all(&dbpool).await?;
                if results.is_empty() {
                    return Err(anyhow!("No results found"));
                }
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.set_header(["ID", "Rank", "Name", "Common Names", "MN Status"]);
                for row in &results {
                    let mut rank: Option<TaxonRank> = None;
                    let tsn: i64 = row.get("tsn");
                    let rankid = row.get::<i64, _>("rank_id");
                    if let Ok(r) = rankid.try_into() {
                        rank = TaxonRank::from_repr(r);
                    }
                    let s = NativeStatus::from_str(row.get("native_status"))
                        .map(|v| v.to_string())
                        .ok();
                    tbuilder.push_record([
                        tsn.to_string(),
                        rank.map(|r| r.to_string()).unwrap_or("".to_string()),
                        row.get("complete_name"),
                        row.get("cnames"),
                        s.unwrap_or("".to_string()),
                    ]);
                }
                print_table(tbuilder, results.len());
                Ok(())
            }
        },
        None => Err(anyhow!("Missing command")),
    }
}

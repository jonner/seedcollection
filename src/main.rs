use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use log::debug;
use sqlx::SqlitePool;
use std::path::PathBuf;
use tokio;

mod collection;
mod location;
mod sample;
mod taxonomy;

pub fn logger() -> env_logger::Builder {
    let env = env_logger::Env::new()
        .filter_or("SC_LOG", "warn")
        .write_style("SC_LOG_STYLE");
    env_logger::Builder::from_env(env)
}

fn print_table(builder: tabled::builder::Builder, nrecs: usize) {
    use tabled::settings::{object::Segment, width::Width, Modify, Style};
    println!(
        "{}\n",
        builder
            .build()
            .with(Style::psql())
            .with(Modify::new(Segment::all()).with(Width::wrap(60)))
    );
    println!("{} records found", nrecs);
}

async fn print_samples(dbpool: &SqlitePool, collectionid: Option<i64>, full: bool) -> Result<()>
{
    let mut sqlbuilder = sample::build_query(collectionid);
    let samples: Vec<sample::Sample> = sqlbuilder.build_query_as()
        .fetch_all(dbpool)
        .await?;
    let mut tbuilder = tabled::builder::Builder::new();
    let mut headers = vec!["ID", "Taxon", "Location"];
    if full {
        headers.extend_from_slice(&["Month", "Year", "Qty", "Notes"]);
    }
    tbuilder.set_header(headers);
    for sample in &samples {
        let mut vals = vec![
            sample.id.to_string(),
            sample.taxon.complete_name.clone(),
            sample.location.name.clone(),
        ];
        if full {
            vals.push(sample.month.map(|x| x.to_string()).unwrap_or("".to_string()));
            vals.push(sample.year.map(|x| x.to_string()).unwrap_or("".to_string()));
            vals.push(sample.quantity.map(|x| x.to_string()).unwrap_or("".to_string()));
            vals.push(sample.notes.clone().unwrap_or("".to_string()));
        }
        tbuilder.push_record(vals);
    }
    print_table(tbuilder, samples.len());
    Ok(())
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
    Show {
        id: i64,
        #[arg(short, long)]
        full: bool,
    },
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
    List {
        #[arg(short, long)]
        full: bool,
    },
    #[command(about = "Add a new sample to the database")]
    Add {
        #[arg(short, long)]
        taxon: i64,
        #[arg(short, long)]
        location: Option<i64>,
        #[arg(short, long)]
        month: Option<u16>,
        #[arg(short, long)]
        year: Option<u16>,
        #[arg(short, long)]
        quantity: Option<u32>,
        #[arg(short, long)]
        notes: Option<String>,
    },
    #[command(about = "Remove an existing sample from the database")]
    Remove { id: i64 },
    #[command(
        group(
            clap::ArgGroup::new("modify")
                .required(true)
                .multiple(true)
                .args(&["taxon", "location", "month", "year", "quantity", "notes"]),
        ),
        about="Modify properties of a sample")]
    Modify {
        #[arg(long)]
        id: i64,
        #[arg(long)]
        taxon: Option<i64>,
        #[arg(long)]
        location: Option<i64>,
        #[arg(short, long)]
        month: Option<u16>,
        #[arg(short, long)]
        year: Option<u16>,
        #[arg(short, long)]
        quantity: Option<u32>,
        #[arg(short, long)]
        notes: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum TaxonomyCommands {
    #[command(about = "Find a taxon in the database")]
    Find {
        #[arg(long, help = "Only show taxa with the given ID")]
        id: Option<i64>,
        #[arg(long, help = "Only show taxa with the given rank (e.g. 'family')")]
        rank: Option<taxonomy::Rank>,
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

#[tokio::main]
async fn main() -> Result<()> {
    logger().init();
    let args = Cli::parse();
    let dbpool =
        SqlitePool::connect(&format!("sqlite://{}", args.database.to_string_lossy())).await?;
    match args.command {
        Some(Commands::Collection { command }) => match command {
            CollectionCommands::List { full } => {
                let collections: Vec<collection::Collection> = sqlx::query_as(
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
                for collection in &collections {
                    let mut vals = vec![collection.id.to_string(), collection.name.clone()];
                    if full {
                        vals.push(collection.description.clone().unwrap_or("".to_string()));
                    }
                    tbuilder.push_record(vals);
                }
                print_table(tbuilder, collections.len());
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
            CollectionCommands::Show { id, full } => {
                let collectioninfo = sqlx::query!(
                    "SELECT name, description from seedcollections WHERE id=?1",
                    id
                )
                .fetch_one(&dbpool)
                .await?;
                println!("Collection ID: {}", id);
                println!("Collection name: {}", collectioninfo.name);
                if let Some(desc) = &collectioninfo.description {
                    println!("  {}", desc);
                }
                println!("");
                print_samples(&dbpool, Some(id), full).await
            }
        },
        Some(Commands::Location { command }) => match command {
            LocationCommands::List { full } => {
                let locations: Vec<location::Location> = sqlx::query_as("SELECT locid, name as locname, description, latitude, longitude FROM seedlocations")
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
                for loc in &locations {
                    let mut vals = vec![loc.id.to_string(), loc.name.clone()];
                    if full {
                        vals.push(loc.description.clone().unwrap_or("".to_string()));
                        vals.push(
                            loc.latitude
                                .map(|n| n.to_string())
                                .unwrap_or("".to_string()),
                        );
                        vals.push(
                            loc.longitude
                                .map(|n| n.to_string())
                                .unwrap_or("".to_string()),
                        );
                    }
                    tbuilder.push_record(vals);
                }
                print_table(tbuilder, locations.len());
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
            SampleCommands::List { full } => {
                print_samples(&dbpool, None, full).await
            }
            SampleCommands::Add {
                taxon,
                location,
                month,
                year,
                quantity,
                notes,
            } => {
                let newid = sqlx::query!(
                    r#"INSERT INTO seedsamples (tsn, month, year, collectedlocation, quantity, notes)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)"#,
                    taxon,
                    month,
                    year,
                    location,
                    quantity,
                    notes,
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
                taxon,
                location,
                month,
                year,
                quantity,
                notes,
            } => {
                if taxon.is_none()
                    && location.is_none()
                    && month.is_none()
                    && year.is_none()
                    && quantity.is_none()
                    && notes.is_none()
                {
                    return Err(anyhow!("Cannot modify without new values"));
                }
                let mut builder = sqlx::QueryBuilder::new("UPDATE seedsamples SET ");
                let mut sep = builder.separated(", ");
                if let Some(taxon) = taxon {
                    sep.push("tsn = ");
                    sep.push_bind_unseparated(taxon);
                }
                if let Some(location) = location {
                    sep.push("collectedlocation = ");
                    sep.push_bind_unseparated(location);
                }
                if let Some(month) = month {
                    sep.push("month = ");
                    sep.push_bind_unseparated(month);
                }
                if let Some(year) = year {
                    sep.push("year = ");
                    sep.push_bind_unseparated(year);
                }
                if let Some(notes) = notes {
                    sep.push("notes = ");
                    sep.push_bind_unseparated(notes);
                }
                if let Some(quantity) = quantity {
                    sep.push("quantity = ");
                    sep.push_bind_unseparated(quantity);
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
                let mut query = taxonomy::build_query(id, rank, genus, species, any, minnesota);
                let taxa: Vec<taxonomy::Taxon> = query.build_query_as().fetch_all(&dbpool).await?;
                if taxa.is_empty() {
                    return Err(anyhow!("No results found"));
                }
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.set_header(["ID", "Rank", "Name", "Common Names", "MN Status"]);
                for taxon in &taxa {
                    tbuilder.push_record([
                        taxon.id.to_string(),
                        taxon.rank.to_string(),
                        taxon.complete_name.clone(),
                        taxon.vernaculars.join(", "),
                        taxon
                            .native_status
                            .as_ref()
                            .map(|v| v.to_string())
                            .unwrap_or("".to_string()),
                    ]);
                }
                print_table(tbuilder, taxa.len());
                Ok(())
            }
        },
        None => Err(anyhow!("Missing command")),
    }
}

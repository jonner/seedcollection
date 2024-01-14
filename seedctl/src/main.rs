use anyhow::{anyhow, Result};
use clap::Parser;
use cli::*;
use libseed::{
    collection::{AssignedSample, Collection},
    location::{self, Location},
    sample::Sample,
    taxonomy::{self, filter_by, Taxon},
};
use sqlx::SqlitePool;
use tracing::debug;

trait ConstructTableRow {
    fn row_values(&self, full: bool) -> Vec<String>;
}

impl ConstructTableRow for Sample {
    fn row_values(&self, full: bool) -> Vec<String> {
        let mut vals = vec![
            self.id.to_string(),
            self.taxon.complete_name.clone(),
            self.location.name.clone(),
        ];
        if full {
            vals.push(self.month.map(|x| x.to_string()).unwrap_or("".to_string()));
            vals.push(self.year.map(|x| x.to_string()).unwrap_or("".to_string()));
            vals.push(
                self.quantity
                    .map(|x| x.to_string())
                    .unwrap_or("".to_string()),
            );
            vals.push(self.notes.clone().unwrap_or("".to_string()));
        }
        vals
    }
}

impl ConstructTableRow for AssignedSample {
    fn row_values(&self, full: bool) -> Vec<String> {
        let mut vals = vec![self.id.to_string()];
        vals.append(&mut self.sample.row_values(full));
        vals
    }
}

trait ConstructTable {
    type Item: ConstructTableRow;

    fn table_headers(&self, full: bool) -> Vec<&'static str>;
    fn items(&self) -> impl Iterator<Item = &Self::Item>;
    fn construct_table(&self, full: bool) -> Result<(tabled::builder::Builder, usize)> {
        let mut tbuilder = tabled::builder::Builder::new();
        let headers = self.table_headers(full);
        tbuilder.set_header(headers);
        for item in self.items() {
            let vals = item.row_values(full);
            tbuilder.push_record(vals);
        }
        Ok((tbuilder, self.items().count()))
    }
}

fn sample_headers(full: bool) -> Vec<&'static str> {
    let mut headers = vec!["ID", "Taxon", "Location"];
    if full {
        headers.extend_from_slice(&["Month", "Year", "Qty", "Notes"]);
    }
    headers
}

impl ConstructTable for Vec<Sample> {
    type Item = Sample;

    fn table_headers(&self, full: bool) -> Vec<&'static str> {
        sample_headers(full)
    }

    fn items(&self) -> impl Iterator<Item = &Self::Item> {
        self.iter()
    }
}

impl ConstructTable for Collection {
    type Item = AssignedSample;

    fn table_headers(&self, full: bool) -> Vec<&'static str> {
        let mut headers = sample_headers(full);
        headers.insert(0, "ID");
        headers[1] = "SampleID";
        headers
    }

    fn items(&self) -> impl Iterator<Item = &Self::Item> {
        self.samples.iter()
    }
}

mod cli;

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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Cli::parse();
    let dbpool =
        SqlitePool::connect(&format!("sqlite://{}", args.database.to_string_lossy())).await?;
    sqlx::migrate!("../db/migrations").run(&dbpool).await?;
    match args.command {
        Commands::Collection { command } => match command {
            CollectionCommands::List { full } => {
                let collections = Collection::fetch_all(None, &dbpool).await?;
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
                    r#"INSERT INTO sc_collections (name, description)
                VALUES (?1, ?2)"#,
                    name,
                    description,
                )
                .execute(&dbpool)
                .await?
                .last_insert_rowid();
                let row = sqlx::query!(
                    r#"SELECT L.id, L.name, L.description
                                      FROM sc_collections L WHERE id=?"#,
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
                let mut builder = sqlx::QueryBuilder::new("UPDATE sc_collections SET ");
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
                sqlx::query!(r#"DELETE FROM sc_collections WHERE id=?"#, id)
                    .execute(&dbpool)
                    .await?;
                println!("Removed collection {id}");
                Ok(())
            }
            CollectionCommands::AddSample { collection, sample } => {
                sqlx::query!(
                    r#"INSERT INTO sc_collection_samples (collectionid, sampleid) 
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
                    r#"DELETE FROM sc_collection_samples WHERE collectionid=? AND sampleid=?"#,
                    collection,
                    sample,
                )
                .execute(&dbpool)
                .await?;
                println!("Removed sample from collection");
                Ok(())
            }
            CollectionCommands::Show { id, full } => {
                let mut collectioninfo = Collection::fetch(id, &dbpool).await?;
                collectioninfo.fetch_samples(None, &dbpool).await?;
                println!("Collection ID: {}", id);
                println!("Collection name: {}", collectioninfo.name);
                if let Some(desc) = &collectioninfo.description {
                    println!("  {}", desc);
                }
                println!();
                let (builder, nitems) = collectioninfo.construct_table(full)?;
                print_table(builder, nitems);
                Ok(())
            }
        },
        Commands::Location { command } => match command {
            LocationCommands::List { full } => {
                let locations: Vec<location::Location> = sqlx::query_as("SELECT locid, name as locname, description, latitude, longitude FROM sc_locations")
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
                userid,
            } => {
                let location = Location::new(name, description, latitude, longitude, userid);

                let newid = location.insert(&dbpool).await?.last_insert_rowid();
                println!("Added location {newid} to database");
                Ok(())
            }
            LocationCommands::Remove { id } => {
                sqlx::query!(r#"DELETE FROM sc_locations WHERE locid=?1"#, id)
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
                let mut builder = sqlx::QueryBuilder::new("UPDATE sc_locations SET ");
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
        Commands::Sample { command } => match command {
            SampleCommands::List { full } => {
                let samples = Sample::fetch_all(None, &dbpool).await?;
                let (builder, nitems) = samples.construct_table(full)?;
                print_table(builder, nitems);
                Ok(())
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
                    r#"INSERT INTO sc_samples (tsn, month, year, collectedlocation, quantity, notes)
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
                sqlx::query!("DELETE FROM sc_samples WHERE id=?", id,)
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
                let mut builder = sqlx::QueryBuilder::new("UPDATE sc_samples SET ");
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
        Commands::Taxonomy { command } => match command {
            TaxonomyCommands::Find {
                id,
                rank,
                genus,
                species,
                any,
                minnesota,
            } => {
                let minnesota = match minnesota {
                    true => Some(true),
                    false => None,
                };
                let taxa: Vec<Taxon> = Taxon::fetch_all(
                    filter_by(id, rank, genus, species, any, minnesota),
                    None,
                    &dbpool,
                )
                .await?;
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
    }
}

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use inquire::validator::Validation;
use libseed::{
    filter::{Cmp, FilterBuilder, FilterOp},
    loadable::{ExternalRef, Loadable},
    project::{Allocation, Project},
    sample::{self, Certainty, Sample},
    source::Source,
    taxonomy::{filter_by, Taxon},
    user::{User, UserStatus},
};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::{
    io::{stdin, stdout, Write},
    sync::Arc,
};
use tabled::Table;
use tokio::fs;
use tracing::debug;

use crate::cli::*;
use crate::config::*;
use crate::prompt::*;
use crate::table::*;

trait ConstructTableRow {
    fn row_values(&self, full: bool) -> Result<Vec<String>>;
}

impl ConstructTableRow for Sample {
    fn row_values(&self, full: bool) -> Result<Vec<String>> {
        let mut vals = vec![
            self.id.to_string(),
            self.taxon.object()?.complete_name.clone(),
            self.source.object()?.name.clone(),
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
        Ok(vals)
    }
}

impl ConstructTableRow for Allocation {
    fn row_values(&self, full: bool) -> Result<Vec<String>> {
        let mut vals = vec![self.id.to_string()];
        vals.append(&mut self.sample.row_values(full)?);
        Ok(vals)
    }
}

trait ConstructTable {
    type Item: ConstructTableRow;

    fn table_headers(&self, full: bool) -> Vec<&'static str>;
    fn items(&self) -> impl Iterator<Item = &Self::Item>;
    fn construct_table(&self, full: bool) -> Result<tabled::builder::Builder> {
        let mut tbuilder = tabled::builder::Builder::new();
        let headers = self.table_headers(full);
        tbuilder.push_record(headers);
        for item in self.items() {
            let vals = item.row_values(full)?;
            tbuilder.push_record(vals);
        }
        Ok(tbuilder)
    }
}

fn sample_headers(full: bool) -> Vec<&'static str> {
    let mut headers = vec!["ID", "Taxon", "Source"];
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

impl ConstructTable for Project {
    type Item = Allocation;

    fn table_headers(&self, full: bool) -> Vec<&'static str> {
        let mut headers = sample_headers(full);
        headers.insert(0, "ID");
        headers[1] = "SampleID";
        headers
    }

    fn items(&self) -> impl Iterator<Item = &Self::Item> {
        self.allocations.iter()
    }
}

mod cli;
mod config;
mod prompt;
mod table;

fn apply_style(table: &mut tabled::Table) -> &mut Table {
    use tabled::settings::{object::Segment, width::Width, Modify, Style};
    let m = Modify::new(Segment::all()).with(Width::wrap(60).keep_words());
    table.with(m).with(Style::psql())
}

fn print_table(builder: tabled::builder::Builder, show_count: bool) {
    use tabled::settings::{object::Segment, width::Width, Modify, Style};
    let mut table = builder.build();
    println!(
        "{}\n",
        table
            .with(Style::psql())
            .with(Modify::new(Segment::all()).with(Width::wrap(60).keep_words()))
    );
    if show_count {
        let nrecs = table.count_rows();
        println!("{} records found", nrecs);
    }
}

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

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let args = Cli::parse();
    let xdgdirs = xdg::BaseDirectories::new()?;
    let config_file = xdgdirs.place_config_file("seedctl/config")?;
    let cfg = match &args.command {
        Commands::Login { username, database } => {
            let pwd = inquire::Password::new("Password:")
                .with_display_toggle_enabled()
                .with_display_mode(inquire::PasswordDisplayMode::Masked)
                .without_confirmation()
                .prompt()?;
            let cfg = Config::new(username.clone(), pwd, database.clone());
            cfg.save_to_file(&config_file).await.map(|_| cfg)
        }
        _ => config::Config::load_from_file(&config_file).await,
    }
    .with_context(|| "Must log in before issuing any other commands")?;

    debug!(?cfg.username, ?cfg.database, "logging in");
    let dbpool =
        SqlitePool::connect(&format!("sqlite://{}", cfg.database.to_string_lossy())).await?;
    sqlx::migrate!("../db/migrations").run(&dbpool).await?;
    let user = User::fetch_by_username(&cfg.username, &dbpool)
        .await
        .with_context(|| "Failed to fetch user from database".to_string())?
        .ok_or_else(|| anyhow!("Unable to find user {}", cfg.username))?;
    user.verify_password(&cfg.password)?;

    match args.command {
        Commands::Login { .. } => {
            Ok(()) // already handled above
        }
        Commands::Status => {
            println!("Using database '{}'", cfg.database.to_string_lossy());
            println!("Logged in as user '{}'", cfg.username);
            Ok(())
        }
        Commands::Project { command } => match command {
            ProjectCommands::List { full } => {
                let projects = Project::fetch_all(None, &dbpool).await?;
                let mut tbuilder = tabled::builder::Builder::new();
                let mut header = vec!["ID", "Name"];
                if full {
                    header.push("Description");
                }
                tbuilder.push_record(header);
                for project in &projects {
                    let mut vals = vec![project.id.to_string(), project.name.clone()];
                    if full {
                        vals.push(project.description.clone().unwrap_or("".to_string()));
                    }
                    tbuilder.push_record(vals);
                }
                print_table(tbuilder, true);
                Ok(())
            }
            ProjectCommands::Add {
                name,
                description,
                userid,
            } => {
                let mut project = Project::new(name, description, userid.unwrap_or(user.id));
                let id = project.insert(&dbpool).await?.last_insert_rowid();
                let project = Project::fetch(id, &dbpool).await?;
                println!("Added project to database:");
                println!("{}: {}", project.id, project.name);
                Ok(())
            }
            ProjectCommands::Modify {
                id,
                name,
                description,
            } => {
                let mut project = Project::fetch(id, &dbpool).await?;
                if let Some(name) = name {
                    project.name = name
                }
                if let Some(description) = description {
                    project.description = Some(description);
                }
                project.update(&dbpool).await?;
                println!("Modified project...");
                Ok(())
            }
            ProjectCommands::Remove { id } => {
                Project::delete_id(&id, &dbpool).await?;
                println!("Removed project {id}");
                Ok(())
            }
            ProjectCommands::AddSample { project, sample } => {
                let mut project = Project::fetch(project, &dbpool).await?;
                project
                    .allocate_sample(ExternalRef::Stub(sample), &dbpool)
                    .await?;
                println!("Added sample to project");
                Ok(())
            }
            ProjectCommands::RemoveSample { project, sample } => {
                sqlx::query!(
                    r#"DELETE FROM sc_project_samples WHERE projectid=? AND sampleid=?"#,
                    project,
                    sample,
                )
                .execute(&dbpool)
                .await?;
                println!("Removed sample from project");
                Ok(())
            }
            ProjectCommands::Show { id, full } => {
                let mut projectinfo = Project::fetch(id, &dbpool).await?;
                projectinfo.fetch_samples(None, None, &dbpool).await?;
                println!("Project ID: {}", id);
                println!("Project name: {}", projectinfo.name);
                if let Some(desc) = &projectinfo.description {
                    println!("  {}", desc);
                }
                println!();
                let builder = projectinfo.construct_table(full)?;
                print_table(builder, true);
                Ok(())
            }
        },
        Commands::Source { command } => match command {
            SourceCommands::List { full } => {
                let sources = Source::fetch_all(None, &dbpool).await?;
                let mut tbuilder = tabled::builder::Builder::new();
                let mut header = vec!["ID", "Name"];
                if full {
                    header.push("Description");
                    header.push("latitude");
                    header.push("longitude");
                };
                tbuilder.push_record(header);
                for src in &sources {
                    let mut vals = vec![src.id.to_string(), src.name.clone()];
                    if full {
                        vals.push(src.description.clone().unwrap_or("".to_string()));
                        vals.push(
                            src.latitude
                                .map(|n| n.to_string())
                                .unwrap_or("".to_string()),
                        );
                        vals.push(
                            src.longitude
                                .map(|n| n.to_string())
                                .unwrap_or("".to_string()),
                        );
                    }
                    tbuilder.push_record(vals);
                }
                print_table(tbuilder, true);
                Ok(())
            }
            SourceCommands::Show { id } => {
                let src = Source::fetch(id, &dbpool).await?;
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.push_record(["Field", "Value"]);
                tbuilder.push_record(["Id", &src.id.to_string()]);
                tbuilder.push_record(["Name", &src.name]);
                tbuilder.push_record([
                    "Latitude",
                    &src.latitude
                        .map(|l| l.to_string())
                        .unwrap_or("".to_string()),
                ]);
                tbuilder.push_record([
                    "Longitude",
                    &src.longitude
                        .map(|l| l.to_string())
                        .unwrap_or("".to_string()),
                ]);
                tbuilder.push_record(["Description", &src.description.unwrap_or("".to_string())]);
                print_table(tbuilder, false);
                Ok(())
            }
            SourceCommands::Add {
                interactive,
                name,
                description,
                latitude,
                longitude,
                userid,
            } => {
                let mut source = if interactive {
                    let name = inquire::Text::new("Name:").prompt()?;
                    let description = inquire::Text::new("Description:").prompt_skippable()?;
                    let latitude = inquire::CustomType::<f64>::new("Latitude:")
                        .with_validator(|val: &f64| {
                            if *val < -90.0 || *val > 90.0 {
                                return Ok(Validation::Invalid(
                                    "Value must be between -90 and 90".into(),
                                ));
                            }
                            Ok(Validation::Valid)
                        })
                        .prompt_skippable()?;
                    let longitude = inquire::CustomType::<f64>::new("Longitude:")
                        .with_validator(|val: &f64| {
                            if *val < -180.0 || *val > 180.0 {
                                return Ok(Validation::Invalid(
                                    "Value must be betwen -180 and 180".into(),
                                ));
                            }
                            Ok(Validation::Valid)
                        })
                        .prompt_skippable()?;

                    if !inquire::Confirm::new("Save to database?")
                        .with_default(false)
                        .prompt()?
                    {
                        return Err(anyhow!("Aborted"));
                    }

                    Source::new(name, description, latitude, longitude, user.id)
                } else {
                    Source::new(
                        name.ok_or_else(|| anyhow!("No name specified"))?,
                        description,
                        latitude,
                        longitude,
                        userid.unwrap_or(user.id),
                    )
                };

                let newid = source.insert(&dbpool).await?.last_insert_rowid();
                println!("Added source {newid} to database");
                Ok(())
            }
            SourceCommands::Remove { id } => {
                Source::delete_id(&id, &dbpool).await?;
                println!("Removed source {id} from database");
                Ok(())
            }
            SourceCommands::Modify {
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
                    return Err(anyhow!("Cannot modify source without new values"));
                }
                let mut src = Source::fetch(id, &dbpool).await?;
                if let Some(name) = name {
                    src.name = name;
                }
                if let Some(description) = description {
                    src.description = Some(description);
                }
                if let Some(latitude) = latitude {
                    src.latitude = Some(latitude);
                }
                if let Some(longitude) = longitude {
                    src.longitude = Some(longitude);
                }
                src.update(&dbpool).await?;
                println!("Modified source...");
                Ok(())
            }
        },
        Commands::Sample { command } => match command {
            SampleCommands::List {
                full,
                user: useronly,
                limit,
                sort,
            } => {
                let filter = limit.map(|s| {
                    let fbuilder = FilterBuilder::new(FilterOp::Or)
                        .push(Arc::new(sample::Filter::TaxonNameLike(s.clone())))
                        .push(Arc::new(sample::Filter::SourceNameLike(s.clone())))
                        .push(Arc::new(sample::Filter::Notes(
                            libseed::filter::Cmp::Like,
                            s.clone(),
                        )));
                    fbuilder.build()
                });
                let sort = sort.map(|v| match v {
                    SampleSortField::Id => sample::Sort::Id,
                    SampleSortField::Taxon => sample::Sort::TaxonSequence,
                    SampleSortField::Name => sample::Sort::TaxonName,
                    SampleSortField::Source => sample::Sort::SourceName,
                });
                let samples = match useronly {
                    true => Sample::fetch_all_user(user.id, filter, sort, &dbpool).await?,
                    false => Sample::fetch_all(filter, sort, &dbpool).await?,
                };
                let mut table = match full {
                    true => Table::new(
                        samples
                            .iter()
                            .map(|sample| SampleRowFull::new(sample).unwrap()),
                    ),
                    false => {
                        Table::new(samples.iter().map(|sample| SampleRow::new(sample).unwrap()))
                    }
                };
                println!("{}\n", apply_style(&mut table));
                println!("{} records found", table.count_rows());
                Ok(())
            }
            SampleCommands::Show { id } => {
                let sample = Sample::fetch(id, &dbpool).await?;
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.push_record(["Name", "Value"]);
                tbuilder.push_record(["ID", &id.to_string()]);
                let mut taxon = sample.taxon.object()?.clone();
                tbuilder.push_record(["Taxon", &format!("{} ({})", taxon.complete_name, taxon.id)]);
                tbuilder.push_record(["Common Names", &taxon.vernaculars.join("\n")]);
                let src = sample.source.object()?;
                tbuilder.push_record(["Source", &format!("{} ({})", src.name, src.id)]);
                let datestring = match (sample.month, sample.year) {
                    (Some(m), Some(y)) => &format!("{m}/{y}"),
                    (None, Some(y)) => &y.to_string(),
                    _ => "Unknown",
                };
                tbuilder.push_record(["Collection Date", datestring]);
                tbuilder.push_record([
                    "Quantity",
                    &sample
                        .quantity
                        .map(|i| i.to_string())
                        .unwrap_or("".to_string()),
                ]);
                tbuilder.push_record(["Certainty", &sample.certainty.to_string()]);
                taxon.fetch_germination_info(&dbpool).await?;
                if let Some(ref germ) = taxon.germination {
                    tbuilder.push_record([
                        "Germination Codes",
                        &germ
                            .iter()
                            .map(|g| g.code.clone())
                            .collect::<Vec<String>>()
                            .join(", "),
                    ])
                }
                tbuilder.push_record(["Notes", &sample.notes.unwrap_or("".to_string())]);
                print_table(tbuilder, false);
                Ok(())
            }
            SampleCommands::Add {
                interactive,
                taxon,
                source,
                month,
                year,
                quantity,
                notes,
                userid,
                uncertain,
            } => {
                let mut sample = if interactive {
                    let taxon = TaxonIdPrompt::new("Taxon:", &dbpool).prompt()?;
                    let source = SourceIdPrompt::new("Source:", user.id, &dbpool).prompt()?;
                    let month = inquire::CustomType::<u32>::new("Month:").prompt_skippable()?;
                    let year = inquire::CustomType::<u32>::new("Year:").prompt_skippable()?;
                    let quantity =
                        inquire::CustomType::<i64>::new("Quantity:").prompt_skippable()?;
                    let notes = inquire::Text::new("Notes:").prompt_skippable()?;
                    let certainty = match inquire::Confirm::new("Uncertain ID?")
                        .with_default(false)
                        .prompt()?
                    {
                        true => Certainty::Uncertain,
                        _ => Certainty::Certain,
                    };

                    if !inquire::Confirm::new("Save to database?")
                        .with_default(false)
                        .prompt()?
                    {
                        return Err(anyhow!("Aborted"));
                    }

                    Sample::new(
                        taxon, user.id, source, month, year, quantity, notes, certainty,
                    )
                } else {
                    let certainty = match uncertain {
                        true => Certainty::Uncertain,
                        _ => Certainty::Certain,
                    };
                    Sample::new(
                        taxon.ok_or_else(|| anyhow!("Taxon not specified"))?,
                        userid.unwrap_or(user.id),
                        source.ok_or(anyhow!("No source ID provided"))?,
                        month,
                        year,
                        quantity,
                        notes,
                        certainty,
                    )
                };
                let newid = sample.insert(&dbpool).await?.last_insert_rowid();
                println!("Added sample {newid} to database");
                Ok(())
            }
            SampleCommands::Remove { id } => {
                Sample::delete_id(&id, &dbpool).await?;
                Ok(())
            }
            SampleCommands::Modify {
                interactive,
                id,
                taxon,
                source,
                month,
                year,
                quantity,
                notes,
            } => {
                if taxon.is_none()
                    && source.is_none()
                    && month.is_none()
                    && year.is_none()
                    && quantity.is_none()
                    && notes.is_none()
                    && !interactive
                {
                    return Err(anyhow!("Cannot modify without new values"));
                }

                let oldsample = Sample::fetch(id, &dbpool).await?;
                let mut sample = oldsample.clone();
                if interactive {
                    println!("Interactively modifying sample {id}. Press <esc> to skip any field.");
                    let current = sample.taxon.object()?;
                    println!("Current taxon: {}. {}", current.id, current.complete_name);
                    if let Some(id) = TaxonIdPrompt::new("Taxon:", &dbpool).prompt_skippable() {
                        sample.taxon = ExternalRef::Stub(id);
                    }

                    let current = sample.source.object()?;
                    println!("Current source: {}. {}", current.id, current.name);
                    if let Some(id) =
                        SourceIdPrompt::new("Source:", user.id, &dbpool).prompt_skippable()
                    {
                        sample.source = ExternalRef::Stub(id);
                    }

                    println!(
                        "Current month: {}",
                        sample
                            .month
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "<missing>".into())
                    );
                    if let Some(month) =
                        inquire::CustomType::<u32>::new("Month:").prompt_skippable()?
                    {
                        sample.month = Some(month);
                    }

                    println!(
                        "Current year: {}",
                        sample
                            .year
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "<missing>".into())
                    );
                    if let Some(year) =
                        inquire::CustomType::<u32>::new("Year:").prompt_skippable()?
                    {
                        sample.year = Some(year);
                    }

                    println!(
                        "Current quantity: {}",
                        sample
                            .quantity
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "<missing>".into())
                    );
                    if let Some(quantity) =
                        inquire::CustomType::<i64>::new("Quantity:").prompt_skippable()?
                    {
                        sample.quantity = Some(quantity);
                    }

                    println!(
                        "Current notes: {}",
                        sample
                            .notes
                            .as_ref()
                            .cloned()
                            .map(|mut v| {
                                v.insert(0, '\n');
                                v.replace('\n', "\n   ")
                            })
                            .unwrap_or_else(|| "<missing>".into())
                    );
                    if let Some(notes) = inquire::Editor::new("Notes:")
                        .with_predefined_text(
                            sample
                                .notes
                                .as_ref()
                                .map(|v| v.as_str())
                                .unwrap_or_else(|| ""),
                        )
                        .prompt_skippable()?
                    {
                        sample.notes = Some(notes);
                    }

                    println!("Current certainty: {}", sample.certainty);
                    if let Some(val) = inquire::Confirm::new("Uncertain ID?")
                        .with_default(false)
                        .prompt_skippable()?
                    {
                        sample.certainty = match val {
                            true => Certainty::Uncertain,
                            false => Certainty::Certain,
                        };
                    };

                    if !inquire::Confirm::new("Save to database?")
                        .with_default(false)
                        .prompt()?
                    {
                        return Err(anyhow!("Aborted"));
                    }
                } else {
                    if let Some(taxon) = taxon {
                        sample.taxon = ExternalRef::Stub(taxon);
                    }
                    if let Some(source) = source {
                        sample.source = ExternalRef::Stub(source);
                    }
                    if let Some(month) = month {
                        sample.month = Some(month.into());
                    }
                    if let Some(year) = year {
                        sample.year = Some(year.into());
                    }
                    if let Some(notes) = notes {
                        sample.notes = Some(notes);
                    }
                    if let Some(quantity) = quantity {
                        sample.quantity = Some(quantity.into());
                    }
                }
                if oldsample != sample {
                    sample.update(&dbpool).await?;
                    println!("Modified sample...");
                } else {
                    println!("Sample unchanged.")
                }
                Ok(())
            }
        },
        Commands::Taxonomy { command } => match command {
            TaxonomyCommands::Find {
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
                    filter_by(None, rank, genus, species, any, minnesota),
                    None,
                    &dbpool,
                )
                .await?;
                if taxa.is_empty() {
                    return Err(anyhow!("No results found"));
                }
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.push_record(["ID", "Rank", "Name", "Common Names", "MN Status"]);
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
                print_table(tbuilder, true);
                Ok(())
            }
            TaxonomyCommands::Show { id } => {
                let mut taxon = Taxon::fetch(id, &dbpool).await?;
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.push_record(["Property", "Value"]);
                // tbuilder.push_record(["ID", &taxon.id().to_string()]);
                tbuilder.push_record(["Name", &taxon.complete_name]);
                tbuilder.push_record(["Rank", &taxon.rank.to_string()]);
                tbuilder.push_record(["Common Names", &taxon.vernaculars.join("\n")]);
                taxon.fetch_germination_info(&dbpool).await?;
                if let Some(germ) = taxon.germination {
                    tbuilder.push_record([
                        "Germination Codes",
                        &germ
                            .iter()
                            .map(|g| g.code.clone())
                            .collect::<Vec<String>>()
                            .join(", "),
                    ])
                }
                // Figure out how many samples of this taxon in the database:
                let samples = libseed::sample::Sample::fetch_all_user(
                    user.id,
                    Some(Arc::new(sample::Filter::TaxonId(Cmp::Equal, id))),
                    None,
                    &dbpool,
                )
                .await?;
                tbuilder.push_record([
                    "Samples",
                    &samples
                        .iter()
                        .map(|s| s.id.to_string())
                        .collect::<Vec<String>>()
                        .join("\n"),
                ]);
                print_table(tbuilder, false);
                Ok(())
            }
        },
        Commands::User { command } => match command {
            UserCommands::List {} => {
                let users = User::fetch_all(&dbpool).await?;
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.push_record(["ID", "Username", "Email"]);
                for user in &users {
                    tbuilder.push_record([
                        user.id.to_string(),
                        user.username.clone(),
                        user.email.clone(),
                    ]);
                }
                print_table(tbuilder, true);
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
                let id = user.insert(&dbpool).await?.last_insert_rowid();
                println!("Added user to database:");
                println!("{}: {}", id, username);
                Ok(())
            }
            UserCommands::Remove { id } => User::delete_id(&id, &dbpool)
                .await
                .map(|_| ())
                .with_context(|| "failed to remove user"),
            UserCommands::Modify {
                id,
                username,
                change_password,
                password_file,
            } => {
                let mut user = User::fetch(id, &dbpool).await?;
                if let Some(username) = username {
                    user.username = username;
                }
                if change_password {
                    let password = get_password(password_file, None).await?;
                    user.change_password(&password)?;
                }
                user.update(&dbpool)
                    .await
                    .map(|_| ())
                    .with_context(|| "Failed to modify user")
            }
        },
    }
}

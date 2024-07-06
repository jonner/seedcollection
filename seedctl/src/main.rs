use anyhow::{anyhow, Context, Result};
use clap::Parser;
use cli::*;
use config::*;
use inquire::{autocompletion::Autocomplete, validator::Validation, CustomUserError};
use libseed::{
    filter::{Cmp, FilterBuilder, FilterOp},
    loadable::{ExternalRef, Loadable},
    project::{Allocation, Project},
    sample::{Certainty, Sample},
    source::{self, Source},
    taxonomy::{filter_by, quickfind, Taxon},
    user::{User, UserStatus},
};
use sqlx::{Pool, Sqlite, SqlitePool};
use std::io::{stdin, stdout, Write};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tracing::debug;

#[derive(Clone)]
struct TaxonCompleter {
    dbpool: Pool<Sqlite>,
}

impl Autocomplete for TaxonCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut taxa = Ok(vec![]);
        if input.len() > 2 {
            taxa = futures::executor::block_on(Taxon::fetch_all(
                quickfind(input.to_string()),
                None,
                &self.dbpool,
            ));
        }
        taxa.map(|taxa| {
            taxa.iter()
                .map(|t| {
                    let mut cnames = t.vernaculars.join(", ");
                    if !cnames.is_empty() {
                        cnames = format!(" - {cnames}");
                    }
                    format!("{:6}. {}{}", t.id(), t.complete_name.clone(), cnames)
                })
                .collect::<Vec<String>>()
        })
        .map_err(|e| e.into())
    }

    fn get_completion(
        &mut self,
        _input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, CustomUserError> {
        {
            Ok(highlighted_suggestion)
        }
    }
}

#[derive(Clone)]
struct SourceCompleter {
    dbpool: Pool<Sqlite>,
    userid: i64,
}

impl Autocomplete for SourceCompleter {
    fn get_suggestions(&mut self, input: &str) -> Result<Vec<String>, CustomUserError> {
        let mut fbuilder = FilterBuilder::new(FilterOp::And);
        fbuilder = fbuilder.push(Arc::new(source::Filter::UserId(self.userid)));
        fbuilder = fbuilder.push(Arc::new(source::Filter::Name(Cmp::Like, input.to_string())));
        let mut sources = Ok(vec![]);
        if input.len() > 2 {
            sources = futures::executor::block_on(Source::fetch_all(
                Some(fbuilder.build()),
                &self.dbpool,
            ));
        }
        sources
            .map(|taxa| {
                taxa.iter()
                    .map(|src| format!("{}. {}", src.id(), src.name))
                    .collect::<Vec<String>>()
            })
            .map_err(|e| e.into())
    }

    fn get_completion(
        &mut self,
        _input: &str,
        highlighted_suggestion: Option<String>,
    ) -> Result<inquire::autocompletion::Replacement, CustomUserError> {
        {
            Ok(highlighted_suggestion)
        }
    }
}

fn extract_dbid(s: &str) -> Result<i64> {
    let splits = s.split(".");
    for split in splits {
        let split = split.trim();
        println!("<<{split}>>");
    }
    //println!("{:?}", s.split("."));
    s.split(".")
        .next()
        .map(|s| s.trim().parse::<i64>())
        .ok_or_else(|| anyhow!("Couldn't find taxon ID"))?
        .map_err(|e| e.into())
}

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
    fn construct_table(&self, full: bool) -> Result<(tabled::builder::Builder, usize)> {
        let mut tbuilder = tabled::builder::Builder::new();
        let headers = self.table_headers(full);
        tbuilder.set_header(headers);
        for item in self.items() {
            let vals = item.row_values(full)?;
            tbuilder.push_record(vals);
        }
        Ok((tbuilder, self.items().count()))
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
        .with_context(|| format!("Failed to fetch user from database"))?
        .ok_or_else(|| anyhow!("Unable to find user {}", cfg.username))?;
    user.verify_password(&cfg.password)?;

    match args.command {
        Commands::Login { .. } => {
            Ok(()) // already handled above
        }
        Commands::Project { command } => match command {
            ProjectCommands::List { full } => {
                let projects = Project::fetch_all(None, &dbpool).await?;
                let mut tbuilder = tabled::builder::Builder::new();
                let mut header = vec!["ID", "Name"];
                if full {
                    header.push("Description");
                }
                tbuilder.set_header(header);
                for project in &projects {
                    let mut vals = vec![project.id.to_string(), project.name.clone()];
                    if full {
                        vals.push(project.description.clone().unwrap_or("".to_string()));
                    }
                    tbuilder.push_record(vals);
                }
                print_table(tbuilder, projects.len());
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
                let (builder, nitems) = projectinfo.construct_table(full)?;
                print_table(builder, nitems);
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
                tbuilder.set_header(header);
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
                print_table(tbuilder, sources.len());
                Ok(())
            }
            SourceCommands::Add {
                name,
                description,
                latitude,
                longitude,
                userid,
            } => {
                let mut source = Source::new(
                    name,
                    description,
                    latitude,
                    longitude,
                    userid.unwrap_or(user.id),
                );

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
            SampleCommands::List { full } => {
                let samples = Sample::fetch_all(None, &dbpool).await?;
                let (builder, nitems) = samples.construct_table(full)?;
                print_table(builder, nitems);
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
                    let taxon_string = inquire::Text::new("Taxon: ")
                        .with_autocomplete(TaxonCompleter {
                            dbpool: dbpool.clone(),
                        })
                        .prompt()?;
                    debug!(?taxon_string, "got taxon string");
                    // HACK: the autocompleter puts the database ID at the beginning of the string. If it's not there, it's an error.
                    let taxonid = extract_dbid(&taxon_string)?;

                    let source_string = inquire::Text::new("Source: ")
                        .with_autocomplete(SourceCompleter {
                            dbpool: dbpool.clone(),
                            userid: user.id,
                        })
                        .prompt()?;
                    debug!(?source_string, "got source string");
                    // HACK
                    let srcid = extract_dbid(&source_string)?;

                    let month = inquire::Text::new("Month:")
                        .with_validator(|input: &str| match input.parse::<u32>() {
                            Ok(n) if n <= 12 => Ok(Validation::Valid),
                            Ok(_) => Ok(Validation::Invalid("Invalid value for month".into())),
                            Err(_) => Ok(Validation::Invalid("Month should be an integer".into())),
                        })
                        .prompt_skippable()?
                        .map(|s| s.parse::<u32>().unwrap());

                    let year = inquire::Text::new("Year:")
                        .with_validator(|input: &str| match input.parse::<u32>() {
                            Ok(_) => Ok(Validation::Valid),
                            Err(_) => Ok(Validation::Invalid("Year should be an integer".into())),
                        })
                        .prompt_skippable()?
                        .map(|s| s.parse::<u32>().unwrap());
                    let qty = inquire::Text::new("Quantity:")
                        .with_validator(|input: &str| match input.parse::<i64>() {
                            Ok(_) => Ok(Validation::Valid),
                            Err(_) => {
                                Ok(Validation::Invalid("Quantity should be an integer".into()))
                            }
                        })
                        .prompt_skippable()?;

                    let notes = inquire::Text::new("Notes:").prompt_skippable()?;
                    let certainty = match inquire::Confirm::new("Uncertain ID?")
                        .with_default(false)
                        .prompt()?
                    {
                        true => Certainty::Uncertain,
                        _ => Certainty::Certain,
                    };

                    Sample::new(
                        taxonid, user.id, srcid, month, year, quantity, notes, certainty,
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
                {
                    return Err(anyhow!("Cannot modify without new values"));
                }
                let mut sample = Sample::fetch(id, &dbpool).await?;
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
                sample.update(&dbpool).await?;
                println!("Modified sample...");
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
        Commands::User { command } => match command {
            UserCommands::List {} => {
                let users = User::fetch_all(&dbpool).await?;
                let mut tbuilder = tabled::builder::Builder::new();
                tbuilder.set_header(["ID", "Username", "Email"]);
                for user in &users {
                    tbuilder.push_record([
                        user.id.to_string(),
                        user.username.clone(),
                        user.email.clone(),
                    ]);
                }
                print_table(tbuilder, users.len());
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
    .map_err(|e| e.into())
}

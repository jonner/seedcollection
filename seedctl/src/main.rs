use anyhow::{anyhow, Context, Result};
use clap::Parser;
use inquire::validator::Validation;
use libseed::{
    filter::{FilterBuilder, FilterOp},
    loadable::{ExternalRef, Loadable},
    project::Project,
    sample::{self, Certainty, Sample},
    source::Source,
    taxonomy::{filter_by, Germination, Taxon},
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
use crate::table::SeedctlTable;
use crate::table::*;

mod cli;
mod config;
mod prompt;
mod table;

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
            ProjectCommands::List {} => {
                let projects = Project::fetch_all(None, &dbpool).await?;
                let mut table = Table::new(projects.iter().map(|val| ProjectRow::new(&val)));
                println!("{}\n", table.styled());
                println!("{} records found", table.count_rows());
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
                let mut table = match full {
                    true => Table::new(
                        projectinfo
                            .allocations
                            .iter()
                            .map(|alloc| AllocationRowFull::new(alloc).unwrap()),
                    ),
                    false => Table::new(
                        projectinfo
                            .allocations
                            .iter()
                            .map(|alloc| AllocationRow::new(alloc).unwrap()),
                    ),
                };
                println!("{}\n", table.styled());
                println!("{} records found", table.count_rows());
                Ok(())
            }
        },
        Commands::Source { command } => match command {
            SourceCommands::List { full } => {
                let sources = Source::fetch_all(None, &dbpool).await?;
                let mut table = match full {
                    true => Table::new(sources.iter().map(|src| SourceRowFull::new(src))),
                    false => Table::new(sources.iter().map(|src| SourceRow::new(src))),
                };
                println!("{}\n", table.styled());
                println!("{} records found", table.count_rows());
                Ok(())
            }
            SourceCommands::Show { id } => {
                let src = Source::fetch(id, &dbpool).await?;
                let tbuilder = Table::builder(vec![SourceRowFull::new(&src)])
                    .index()
                    .column(0)
                    .transpose();
                println!("{}\n", tbuilder.build().styled());
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
                println!("{}\n", table.styled());
                println!("{} records found", table.count_rows());
                Ok(())
            }
            SampleCommands::Show { id } => {
                let mut sample = Sample::fetch(id, &dbpool).await?;

                let tbuilder =
                    Table::builder(vec![SampleRowDetails::new(&mut sample, &dbpool).await?])
                        .index()
                        .column(0)
                        .transpose();
                println!("{}\n", tbuilder.build().styled());
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
                let mut table = Table::new(taxa.iter().map(|t| TaxonRow::new(t)));
                println!("{}\n", table.styled());
                println!("{} records found", table.count_rows());
                Ok(())
            }
            TaxonomyCommands::Show { id } => {
                let mut taxon = Taxon::fetch(id, &dbpool).await?;
                let tbuilder =
                    Table::builder(vec![TaxonRowDetails::new(&mut taxon, &dbpool).await?])
                        .index()
                        .column(0)
                        .transpose();
                println!("{}\n", tbuilder.build().styled());
                Ok(())
            }
        },
        Commands::Admin { command } => match command {
            AdminCommands::User { command } => match command {
                UserCommands::List {} => {
                    let users = User::fetch_all(&dbpool).await?;
                    let mut table = Table::new(users.iter().map(|u| UserRow::new(u)));
                    println!("{}\n", table.styled());
                    println!("{} records found", table.count_rows());
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
            AdminCommands::Germination { command } => match command {
                GerminationCommands::List {} => {
                    let codes = Germination::fetch_all(&dbpool).await?;
                    let mut table = Table::new(codes.iter().map(|g| GerminationRow::new(g)));
                    println!("{}\n", table.styled());
                    Ok(())
                }
                GerminationCommands::Modify {
                    id,
                    interactive,
                    code,
                    summary,
                    description,
                } => {
                    let oldval = Germination::fetch(id, &dbpool).await?;
                    let mut newval = oldval.clone();
                    if interactive {
                        println!("Modifying Germination code {id}. Pres <esc to skip any field.");
                        println!("Current code: '{}'", oldval.code);
                        if let Some(code) = inquire::Text::new("Code:").prompt_skippable()? {
                            newval.code = code;
                        }
                        println!(
                            "Current summary: '{}'",
                            oldval
                                .summary
                                .as_ref()
                                .cloned()
                                .unwrap_or_else(|| "<null>".to_string())
                        );
                        if let Some(summary) = inquire::Text::new("Summary:").prompt_skippable()? {
                            newval.summary = Some(summary);
                        }
                        println!(
                            "Current description: '{}'",
                            oldval
                                .description
                                .as_ref()
                                .cloned()
                                .unwrap_or_else(|| "<null>".to_string())
                        );
                        if let Some(description) = inquire::Editor::new("Description:")
                            .with_predefined_text(
                                oldval
                                    .description
                                    .as_ref()
                                    .map(|v| v.as_str())
                                    .unwrap_or_else(|| ""),
                            )
                            .prompt_skippable()?
                        {
                            newval.description = Some(description);
                        }
                    } else {
                        if code.is_none() && summary.is_none() && description.is_none() {
                            return Err(anyhow!(
                                "No fields specified. Cannot modify germination code."
                            ));
                        }
                        if let Some(code) = code {
                            newval.code = code;
                        }
                        if let Some(summary) = summary {
                            newval.summary = Some(summary);
                        }
                        if let Some(description) = description {
                            newval.description = Some(description);
                        }
                    }
                    if oldval != newval {
                        debug!("Submitting new value for germination code: {:?}", newval);
                        newval.update(&dbpool).await?;
                        println!("Modified germination code...");
                    }
                    Ok(())
                }
            },
        },
    }
}

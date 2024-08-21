use crate::{
    cli::SourceCommands,
    output::rows::{SourceRow, SourceRowFull},
    table::SeedctlTable,
};
use anyhow::{anyhow, Result};
use inquire::validator::Validation;
use libseed::{
    filter::{Cmp, CompoundFilter, Op},
    loadable::Loadable,
    source::{self, Source},
    user::User,
    Error::{AuthUserNotFound, DatabaseRowNotFound},
};
use sqlx::{Pool, Sqlite};
use tabled::Table;

pub async fn handle_command(
    command: SourceCommands,
    user: User,
    dbpool: &Pool<Sqlite>,
) -> Result<()> {
    match command {
        SourceCommands::List { full, filter } => {
            let filter = filter.map(|f| {
                CompoundFilter::builder(Op::Or)
                    .push(source::Filter::Name(Cmp::Like, f.clone()))
                    .push(source::Filter::Description(Cmp::Like, f.clone()))
                    .build()
            });
            let sources = Source::load_all(filter, dbpool).await?;
            let mut table = match full {
                true => Table::new(sources.iter().map(SourceRowFull::new)),
                false => Table::new(sources.iter().map(SourceRow::new)),
            };
            println!("{}\n", table.styled());
            println!("{} records found", sources.len());
            Ok(())
        }
        SourceCommands::Show { id } => match Source::load(id, dbpool).await {
            Ok(src) => {
                let tbuilder = Table::builder(vec![SourceRowFull::new(&src)])
                    .index()
                    .column(0)
                    .transpose();
                println!("{}\n", tbuilder.build().styled());
                Ok(())
            }
            Err(DatabaseRowNotFound(_)) => {
                println!("Source {id} not found");
                Ok(())
            }
            Err(e) => Err(e.into()),
        },
        SourceCommands::Add {
            name,
            description,
            latitude,
            longitude,
            userid,
        } => {
            let userid = match userid {
                // check if the given userid is valid
                Some(id) => {
                    let _ = User::load(id, dbpool)
                        .await
                        .map_err(|_e| AuthUserNotFound)?;
                    id
                }
                None => user.id,
            };
            let mut source = if name.is_none()
                && description.is_none()
                && latitude.is_none()
                && longitude.is_none()
            {
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

                Source::new(name, description, latitude, longitude, userid)
            } else {
                Source::new(
                    name.ok_or_else(|| anyhow!("No name specified"))?,
                    description,
                    latitude,
                    longitude,
                    userid,
                )
            };

            let newid = source.insert(dbpool).await?.last_insert_rowid();
            println!("Added source {newid} to database");
            Ok(())
        }
        SourceCommands::Remove { id } => {
            Source::delete_id(&id, dbpool).await?;
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
            if name.is_none() && description.is_none() && latitude.is_none() && longitude.is_none()
            {
                return Err(anyhow!("Cannot modify source without new values"));
            }
            let mut src = Source::load(id, dbpool).await?;
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
            src.update(dbpool).await?;
            println!("Modified source...");
            Ok(())
        }
    }
}

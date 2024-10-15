use crate::{
    cli::SourceCommands,
    output::{
        self,
        rows::{SourceRow, SourceRowFull},
    },
};
use anyhow::{anyhow, Result};
use inquire::validator::Validation;
use libseed::{
    loadable::Loadable,
    query::{Cmp, CompoundFilter, Op},
    source::{self, Source},
    user::User,
    Database,
    Error::{AuthUserNotFound, DatabaseError},
};

pub(crate) async fn handle_command(
    command: SourceCommands,
    user: User,
    db: &Database,
) -> Result<()> {
    match command {
        SourceCommands::List {
            filter,
            output,
            user: useronly,
        } => {
            let filter = filter.map(|f| {
                CompoundFilter::builder(Op::Or)
                    .push(source::Filter::Name(Cmp::Like, f.clone()))
                    .push(source::Filter::Description(Cmp::Like, f.clone()))
                    .build()
            });
            let sources = match useronly {
                true => Source::load_all_user(user.id, filter, db).await?,
                false => Source::load_all(filter, db).await?,
            };
            let str = match output.full {
                true => {
                    let rows = sources.iter().map(SourceRowFull::new).collect::<Vec<_>>();
                    output::format_seq(rows, output.format)?
                }
                false => {
                    let rows = sources.iter().map(SourceRow::new).collect::<Vec<_>>();
                    output::format_seq(rows, output.format)?
                }
            };
            println!("{str}");
            Ok(())
        }
        SourceCommands::Show { id, output } => match Source::load(id, db).await {
            Ok(src) => {
                let str = output::format_one(SourceRowFull::new(&src), output.format)?;
                println!("{str}");
                Ok(())
            }
            Err(DatabaseError(sqlx::Error::RowNotFound)) => {
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
                    let _ = User::load(id, db).await.map_err(|_e| AuthUserNotFound)?;
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

            let newid = source.insert(db).await?.last_insert_rowid();
            println!("Added source {newid} to database");
            Ok(())
        }
        SourceCommands::Remove { id } => {
            Source::delete_id(&id, db).await?;
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
            let mut src = Source::load(id, db).await?;
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
            src.update(db).await?;
            println!("Modified source...");
            Ok(())
        }
    }
}

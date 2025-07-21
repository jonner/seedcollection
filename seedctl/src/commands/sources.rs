//! Commands related to seed [Source]s
use crate::{
    cli::SourceCommands,
    output::{
        self,
        rows::{SourceRow, SourceRowFull},
    },
    prompt::prompt_source,
};
use anyhow::{Result, anyhow};
use libseed::{
    Error::{AuthUserNotFound, DatabaseError},
    core::{
        database::Database,
        loadable::Loadable,
        query::filter::{Cmp, or},
    },
    source::{self, Source},
    user::User,
};

/// Handle the `seedctl sources` command and its subcommands
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
                or().push(source::Filter::Name(Cmp::Like, f.clone()))
                    .push(source::Filter::Description(Cmp::Like, f.clone()))
                    .build()
            });
            let sources = match useronly {
                true => Source::load_all_user(user.id, filter, db).await?,
                false => Source::load_all(filter, None, None, db).await?,
            };
            let str = match output.full {
                true => {
                    let rows = sources.iter().map(SourceRowFull::new);
                    output::format_seq(rows, output.format)?
                }
                false => {
                    let rows = sources.iter().map(SourceRow::new);
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
                prompt_source(userid)?
            } else {
                Source::new(
                    name.ok_or_else(|| anyhow!("No name specified"))?,
                    description,
                    latitude,
                    longitude,
                    userid,
                )
            };

            let newid = source.insert(db).await?;
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

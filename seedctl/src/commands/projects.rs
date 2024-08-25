use crate::{
    cli::ProjectCommands,
    output::{
        self,
        rows::{AllocationRow, AllocationRowFull, ProjectRow},
    },
};
use anyhow::Result;
use libseed::{
    loadable::{ExternalRef, Loadable},
    project::Project,
    user::User,
    Error::DatabaseRowNotFound,
};
use sqlx::{Pool, Sqlite};

pub async fn handle_command(
    command: ProjectCommands,
    user: User,
    dbpool: &Pool<Sqlite>,
) -> Result<()> {
    match command {
        ProjectCommands::List { output } => {
            let projects = Project::load_all(None, dbpool).await?;
            let str = output::format_seq(
                projects.iter().map(ProjectRow::new).collect::<Vec<_>>(),
                output.format,
            )?;
            println!("{str}");
            Ok(())
        }
        ProjectCommands::Add {
            name,
            description,
            userid,
        } => {
            let mut project = Project::new(name, description, userid.unwrap_or(user.id));
            let id = project.insert(dbpool).await?.last_insert_rowid();
            let project = Project::load(id, dbpool).await?;
            println!("Added project to database:");
            println!("{}: {}", project.id, project.name);
            Ok(())
        }
        ProjectCommands::Modify {
            id,
            name,
            description,
        } => {
            let mut project = Project::load(id, dbpool).await?;
            if let Some(name) = name {
                project.name = name
            }
            if let Some(description) = description {
                project.description = Some(description);
            }
            project.update(dbpool).await?;
            println!("Modified project...");
            Ok(())
        }
        ProjectCommands::Remove { id } => {
            Project::delete_id(&id, dbpool).await?;
            println!("Removed project {id}");
            Ok(())
        }
        ProjectCommands::AddSample { project, sample } => {
            let mut project = Project::load(project, dbpool).await?;
            project
                .allocate_sample(ExternalRef::Stub(sample), dbpool)
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
            .execute(dbpool)
            .await?;
            println!("Removed sample from project");
            Ok(())
        }
        ProjectCommands::Show { id, output } => match Project::load(id, dbpool).await {
            Ok(mut projectinfo) => {
                projectinfo.load_samples(None, None, dbpool).await?;
                let str = match output.full {
                    true => output::format_seq(
                        projectinfo
                            .allocations
                            .iter()
                            .map(|alloc| AllocationRowFull::new(alloc))
                            .collect::<Result<Vec<_>, _>>()?,
                        output.format,
                    )?,
                    false => output::format_seq(
                        projectinfo
                            .allocations
                            .iter()
                            .map(|alloc| AllocationRow::new(alloc))
                            .collect::<Result<Vec<_>, _>>()?,
                        output.format,
                    )?,
                };
                println!("{str}");
                Ok(())
            }
            Err(DatabaseRowNotFound(_)) => {
                println!("Project {id} not found");
                Ok(())
            }
            Err(e) => Err(e.into()),
        },
    }
}

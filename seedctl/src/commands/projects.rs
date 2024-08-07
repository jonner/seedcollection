use crate::{
    cli::ProjectCommands,
    table::{AllocationRow, AllocationRowFull, ProjectRow, SeedctlTable},
};
use anyhow::Result;
use libseed::{
    loadable::{ExternalRef, Loadable},
    project::Project,
    user::User,
    Error::DatabaseRowNotFound,
};
use sqlx::{Pool, Sqlite};
use tabled::Table;

pub async fn handle_command(
    command: ProjectCommands,
    user: User,
    dbpool: &Pool<Sqlite>,
) -> Result<()> {
    match command {
        ProjectCommands::List {} => {
            let projects = Project::load_all(None, dbpool).await?;
            let mut table = Table::new(projects.iter().map(ProjectRow::new));
            println!("{}\n", table.styled());
            println!("{} records found", projects.len());
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
        ProjectCommands::Show { id, full } => match Project::load(id, dbpool).await {
            Ok(mut projectinfo) => {
                projectinfo.load_samples(None, None, dbpool).await?;
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
                println!("{} records found", projectinfo.allocations.len());
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

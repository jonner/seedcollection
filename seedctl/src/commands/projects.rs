//! Comamnds related to [Project]s
use crate::{
    cli::ProjectCommands,
    output::{
        self,
        rows::{AllocationRow, AllocationRowFull, ProjectRow},
    },
};
use anyhow::Result;
use libseed::{
    core::{
        database::Database,
        loadable::{ExternalRef, Loadable},
        query::{CompoundFilter, Op},
    },
    project::{Allocation, Project, allocation},
    user::User,
};

/// Handle the `seedctl projects` command and its subcommands
pub(crate) async fn handle_command(
    command: ProjectCommands,
    user: User,
    db: &Database,
) -> Result<()> {
    match command {
        ProjectCommands::List { output } => {
            let projects = Project::load_all(None, db).await?;
            let str = output::format_seq(projects.iter().map(ProjectRow::new), output.format)?;
            println!("{str}");
            Ok(())
        }
        ProjectCommands::Add {
            name,
            description,
            userid,
        } => {
            let mut project = Project::new(name, description, userid.unwrap_or(user.id));
            let id = project.insert(db).await?;
            let project = Project::load(id, db).await?;
            println!("Added project to database:");
            println!("{}: {}", project.id, project.name);
            Ok(())
        }
        ProjectCommands::Modify {
            id,
            name,
            description,
        } => {
            let mut project = Project::load(id, db).await?;
            if let Some(name) = name {
                project.name = name
            }
            if let Some(description) = description {
                project.description = Some(description);
            }
            project.update(db).await?;
            println!("Modified project...");
            Ok(())
        }
        ProjectCommands::Remove { id } => {
            Project::delete_id(&id, db).await?;
            println!("Removed project {id}");
            Ok(())
        }
        ProjectCommands::AddSample { project, sample } => {
            let mut project = Project::load(project, db).await?;
            project
                .allocate_sample(ExternalRef::Stub(sample), db)
                .await?;
            println!("Added sample to project");
            Ok(())
        }
        ProjectCommands::RemoveSample { project, sample } => {
            let fb = CompoundFilter::builder(Op::And)
                .push(allocation::Filter::ProjectId(project))
                .push(allocation::Filter::SampleId(sample));
            let mut alloc = Allocation::load_one(Some(fb.build()), db).await?;
            alloc.delete(db).await?;
            println!("Removed sample from project");
            Ok(())
        }
        ProjectCommands::Show { id, output } => match Project::load(id, db).await {
            Ok(mut projectinfo) => {
                projectinfo.load_samples(None, None, db).await?;
                let str = match output.full {
                    true => output::format_seq(
                        projectinfo
                            .allocations
                            .iter()
                            .filter_map(|a| AllocationRowFull::new(a).ok()),
                        output.format,
                    )?,
                    false => output::format_seq(
                        projectinfo
                            .allocations
                            .iter()
                            .filter_map(|a| AllocationRow::new(a).ok()),
                        output.format,
                    )?,
                };
                println!("{str}");
                Ok(())
            }
            Err(libseed::Error::DatabaseError(sqlx::Error::RowNotFound)) => {
                println!("Project {id} not found");
                Ok(())
            }
            Err(e) => Err(e.into()),
        },
    }
}

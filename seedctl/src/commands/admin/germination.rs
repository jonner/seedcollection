use crate::{
    cli::GerminationCommands,
    output::{self, rows::GerminationRow},
};

use anyhow::{Result, anyhow};
use libseed::{Database, taxonomy::Germination};
use tracing::debug;

use std::path::PathBuf;

pub(crate) async fn handle_command(
    dbpath: Option<PathBuf>,
    command: GerminationCommands,
) -> Result<()> {
    let db = Database::open(dbpath.ok_or_else(|| anyhow!("No database specified"))?).await?;
    match command {
        GerminationCommands::List { output } => {
            let codes = Germination::load_all(&db).await?;
            let str = output::format_seq(codes.iter().map(GerminationRow::new), output.format)?;
            println!("{str}");
            Ok(())
        }
        GerminationCommands::Modify {
            id,
            code,
            summary,
            description,
        } => {
            let oldval = Germination::load(id, &db).await?;
            let mut newval = oldval.clone();
            if code.is_none() && summary.is_none() && description.is_none() {
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
                    .with_predefined_text(oldval.description.as_deref().unwrap_or_default())
                    .prompt_skippable()?
                {
                    newval.description = Some(description);
                }
            } else {
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
                newval.update(&db).await?;
                println!("Modified germination code...");
            }
            Ok(())
        }
    }
}

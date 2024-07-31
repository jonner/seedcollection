use crate::{
    cli::{SampleCommands, SampleSortField},
    prompt::{SourceIdPrompt, TaxonIdPrompt},
    table::{SampleRow, SampleRowDetails, SampleRowFull, SeedctlTable},
};
use anyhow::{anyhow, Result};
use libseed::{
    filter::{CompoundFilter, Op},
    loadable::{ExternalRef, Loadable},
    sample::{self, Certainty, Sample},
    user::User,
    Error::{AuthUserNotFound, DatabaseRowNotFound},
};
use sqlx::{Pool, Sqlite};
use tabled::Table;

pub async fn handle_command(
    command: SampleCommands,
    user: User,
    dbpool: &Pool<Sqlite>,
) -> Result<()> {
    match command {
        SampleCommands::List {
            full,
            user: useronly,
            limit,
            sort,
        } => {
            let filter = limit.map(|s| {
                let fbuilder = CompoundFilter::builder(Op::Or)
                    .push(sample::Filter::TaxonNameLike(s.clone()))
                    .push(sample::Filter::SourceNameLike(s.clone()))
                    .push(sample::Filter::Notes(libseed::filter::Cmp::Like, s.clone()));
                fbuilder.build()
            });
            let sort = sort.map(|v| match v {
                SampleSortField::Id => sample::Sort::Id,
                SampleSortField::Taxon => sample::Sort::TaxonSequence,
                SampleSortField::Name => sample::Sort::TaxonName,
                SampleSortField::Source => sample::Sort::SourceName,
            });
            let samples = match useronly {
                true => Sample::load_all_user(user.id, filter, sort, dbpool).await?,
                false => Sample::load_all(filter, sort, dbpool).await?,
            };
            let mut table = match full {
                true => Table::new(
                    samples
                        .iter()
                        .map(|sample| SampleRowFull::new(sample).unwrap()),
                ),
                false => Table::new(samples.iter().map(|sample| SampleRow::new(sample).unwrap())),
            };
            println!("{}\n", table.styled());
            println!("{} records found", samples.len());
            Ok(())
        }
        SampleCommands::Show { id } => match Sample::load(id, dbpool).await {
            Ok(mut sample) => {
                let tbuilder =
                    Table::builder(vec![SampleRowDetails::new(&mut sample, dbpool).await?])
                        .index()
                        .column(0)
                        .transpose();
                println!("{}\n", tbuilder.build().styled());
                Ok(())
            }
            Err(DatabaseRowNotFound(_)) => {
                println!("Sample {id} not found");
                Ok(())
            }
            Err(e) => Err(e.into()),
        },
        SampleCommands::Add {
            taxon,
            source,
            month,
            year,
            quantity,
            notes,
            uncertain,
            userid,
        } => {
            let userid = match userid {
                Some(id) => {
                    let _ = User::load(id, &dbpool)
                        .await
                        .map_err(|_| AuthUserNotFound)?;
                    id
                }
                None => user.id,
            };
            let mut sample = if taxon.is_none()
                && source.is_none()
                && month.is_none()
                && year.is_none()
                && quantity.is_none()
                && notes.is_none()
                && !uncertain
            {
                let taxon = TaxonIdPrompt::new("Taxon:", dbpool).prompt()?;
                let source = SourceIdPrompt::new("Source:", userid, dbpool).prompt()?;
                let month = inquire::CustomType::<u32>::new("Month:").prompt_skippable()?;
                let year = inquire::CustomType::<u32>::new("Year:").prompt_skippable()?;
                let quantity = inquire::CustomType::<i64>::new("Quantity:").prompt_skippable()?;
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
                    taxon, userid, source, month, year, quantity, notes, certainty,
                )
            } else {
                let certainty = match uncertain {
                    true => Certainty::Uncertain,
                    _ => Certainty::Certain,
                };
                Sample::new(
                    taxon.ok_or_else(|| anyhow!("Taxon not specified"))?,
                    userid,
                    source.ok_or(anyhow!("No source ID provided"))?,
                    month,
                    year,
                    quantity,
                    notes,
                    certainty,
                )
            };
            let newid = sample.insert(dbpool).await?.last_insert_rowid();
            println!("Added sample {newid} to database");
            Ok(())
        }
        SampleCommands::Remove { id } => {
            Sample::delete_id(&id, dbpool).await?;
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
            certain,
            uncertain,
        } => {
            let oldsample = Sample::load(id, dbpool).await?;
            let mut sample = oldsample.clone();
            if taxon.is_none()
                && source.is_none()
                && month.is_none()
                && year.is_none()
                && quantity.is_none()
                && notes.is_none()
                && !certain
                && !uncertain
            {
                println!("Interactively modifying sample {id}. Press <esc> to skip any field.");
                let current = sample.taxon.object()?;
                println!("Current taxon: {}. {}", current.id, current.complete_name);
                if let Some(id) = TaxonIdPrompt::new("Taxon:", dbpool).prompt_skippable() {
                    sample.taxon = ExternalRef::Stub(id);
                }

                let current = sample.source.object()?;
                println!("Current source: {}. {}", current.id, current.name);
                if let Some(id) = SourceIdPrompt::new("Source:", user.id, dbpool).prompt_skippable()
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
                if let Some(month) = inquire::CustomType::<u32>::new("Month:").prompt_skippable()? {
                    sample.month = Some(month);
                }

                println!(
                    "Current year: {}",
                    sample
                        .year
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "<missing>".into())
                );
                if let Some(year) = inquire::CustomType::<u32>::new("Year:").prompt_skippable()? {
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
                match (certain, uncertain) {
                    (true, false) => sample.certainty = Certainty::Certain,
                    (false, true) => sample.certainty = Certainty::Uncertain,
                    // should never happen due to cli arg conflict definitions
                    (true, true) => {
                        return Err(anyhow!("Sample cannot be both certain and uncertain"))
                    }
                    _ => (),
                }
            }
            if oldsample != sample {
                sample.update(dbpool).await?;
                println!("Modified sample...");
            } else {
                println!("Sample unchanged.")
            }
            Ok(())
        }
    }
}

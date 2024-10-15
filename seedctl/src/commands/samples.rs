use crate::{
    cli::{SampleCommands, SampleSortField},
    output::{
        self,
        rows::{SampleRow, SampleRowDetails, SampleRowFull},
    },
    prompt::{SourceIdPrompt, TaxonIdPrompt},
    table::SeedctlTable,
};
use anyhow::{anyhow, Result};
use libseed::{
    loadable::{ExternalRef, Loadable},
    query::{Cmp, CompoundFilter, Op, SortOrder, SortSpec, SortSpecs},
    sample::{self, Certainty, Sample},
    user::User,
    Database,
    Error::{AuthUserNotFound, DatabaseError},
};

pub(crate) async fn handle_command(
    command: SampleCommands,
    user: User,
    db: &Database,
) -> Result<()> {
    match command {
        SampleCommands::List {
            user: useronly,
            filter,
            sort,
            reverse,
            all,
            output,
            rank,
        } => {
            let mut builder = CompoundFilter::builder(Op::And);
            if let Some(s) = filter {
                let fbuilder = CompoundFilter::builder(Op::Or)
                    .push(sample::taxon_name_like(s.clone()))
                    .push(sample::Filter::SourceName(Cmp::Like, s.clone()))
                    .push(sample::Filter::Notes(Cmp::Like, s.clone()));
                builder = builder.push(fbuilder.build());
            };
            if !all {
                builder = builder.push(sample::Filter::Quantity(Cmp::NotEqual, 0.0))
            }
            if let Some(rank) = rank {
                builder = builder.push(sample::Filter::TaxonRank(Cmp::GreatherThanEqual, rank))
            }
            let filter = builder.build();
            let sort = sort.map(|vec| {
                let order = match reverse {
                    true => SortOrder::Descending,
                    false => SortOrder::Ascending,
                };
                SortSpecs(
                    vec.iter()
                        .map(|v| match v {
                            SampleSortField::Id => {
                                &[sample::SortField::Id] as &[sample::SortField]
                            }
                            SampleSortField::Taxon => &[sample::SortField::TaxonSequence],
                            SampleSortField::Name => &[sample::SortField::TaxonName],
                            SampleSortField::Source => &[sample::SortField::SourceName],
                            SampleSortField::Date => &[sample::SortField::CollectionDate],
                        })
                        .fold(Vec::new(), |mut acc, val| {
                            acc.extend_from_slice(val);
                            acc
                        })
                        .iter()
                        .map(|field| SortSpec::new(field.clone(), order.clone()))
                        .collect(),
                )
            });
            let samples = match useronly {
                true => Sample::load_all_user(user.id, Some(filter), sort, db).await?,
                false => Sample::load_all(Some(filter), sort, db).await?,
            };
            let str = match output.full {
                true => {
                    let records = samples
                        .into_iter()
                        .map(SampleRowFull::new)
                        .collect::<Result<Vec<_>, _>>()?;
                    output::format_seq(records, output.format)?
                }
                false => {
                    let records = samples
                        .into_iter()
                        .map(SampleRow::new)
                        .collect::<Result<Vec<_>, _>>()?;
                    output::format_seq(records, output.format)?
                }
            };
            println!("{str}",);
            Ok(())
        }
        SampleCommands::Show { id, output } => match Sample::load(id, db).await {
            Ok(mut sample) => {
                let str = output::format_one(
                    SampleRowDetails::new(&mut sample, db).await?,
                    output.format,
                )?;
                println!("{str}");
                Ok(())
            }
            Err(DatabaseError(sqlx::Error::RowNotFound)) => {
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
                    let _ = Sample::load(id, db).await.map_err(|_| AuthUserNotFound)?;
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
                let taxon = TaxonIdPrompt::new("Taxon:", db).prompt()?;
                let source = SourceIdPrompt::new("Source:", userid, db).prompt()?;
                let month = inquire::CustomType::<u8>::new("Month:").prompt_skippable()?;
                let year = inquire::CustomType::<u32>::new("Year:").prompt_skippable()?;
                let quantity =
                    inquire::CustomType::<f64>::new("Quantity (grams):").prompt_skippable()?;
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
            let newid = sample.insert(db).await?.last_insert_rowid();
            println!("Added sample {newid} to database");
            Ok(())
        }
        SampleCommands::Remove { id } => {
            Sample::delete_id(&id, db).await?;
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
            let oldsample = Sample::load(id, db).await?;
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
                if let Some(id) = TaxonIdPrompt::new("Taxon:", db).prompt_skippable() {
                    sample.taxon = ExternalRef::Stub(id);
                }

                let current = sample.source.object()?;
                println!("Current source: {}. {}", current.id, current.name);
                if let Some(id) = SourceIdPrompt::new("Source:", user.id, db).prompt_skippable() {
                    sample.source = ExternalRef::Stub(id);
                }

                println!(
                    "Current month: {}",
                    sample
                        .month
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "<missing>".into())
                );
                if let Some(month) = inquire::CustomType::<u8>::new("Month:").prompt_skippable()? {
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
                    inquire::CustomType::<f64>::new("Quantity (grams):").prompt_skippable()?
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
                    .with_predefined_text(sample.notes.as_deref().unwrap_or_default())
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
                    sample.month = Some(month);
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
                sample.update(db).await?;
                println!("Modified sample...");
            } else {
                println!("Sample unchanged.")
            }
            Ok(())
        }
        SampleCommands::Stats { all } => {
            let filter = match all {
                false => {
                    let mut builder = CompoundFilter::builder(Op::And);
                    builder = builder.push(sample::Filter::Quantity(Cmp::NotEqual, 0.0));
                    Some(builder.build())
                }
                _ => None,
            };
            let stats = Sample::stats(filter, db).await?;
            println!("Collection stats");
            let mut builder = tabled::builder::Builder::new();
            builder.push_record(["Object", "No."]);
            builder.push_record(["Samples", &stats.nsamples.to_string()]);
            builder.push_record(["Taxa", &stats.ntaxa.to_string()]);
            builder.push_record(["Sources", &stats.nsources.to_string()]);
            println!("{}\n", builder.build().styled());
            Ok(())
        }
    }
}

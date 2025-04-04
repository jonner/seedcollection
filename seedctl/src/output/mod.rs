//! Utilities for exporting data from the database
use anyhow::anyhow;
use clap::ValueEnum;
use serde::Serialize;
use table::SeedctlTable;
use tabled::{Table, Tabled};

pub(crate) mod rows;
pub(crate) mod table;

/// Data format for exporting data from the collection
#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub(crate) enum OutputFormat {
    /// Human readable table of data
    Table,
    /// Comma-separated values for importing into a spreadsheet
    Csv,
    /// JSON-formatted objects
    Json,
    /// YAML-formatted objects
    Yaml,
}

/// Serialize a single object into the given data format
pub(crate) fn format_one<T>(item: T, fmt: OutputFormat) -> anyhow::Result<String>
where
    T: Tabled + Serialize + 'static,
{
    match fmt {
        OutputFormat::Table => {
            let tbuilder = Table::builder(vec![item]).index().column(0).transpose();
            Ok(format!("{}", tbuilder.build().styled()))
        }
        OutputFormat::Csv => Err(anyhow!("CSV format is not valid for single items")),
        OutputFormat::Json => serde_json::to_string(&item).map_err(|e| e.into()),
        OutputFormat::Yaml => serde_yaml::to_string(&item).map_err(|e| e.into()),
    }
}

/// Serialize a sequence of objects into the given data format
pub(crate) fn format_seq<I>(items: I, fmt: OutputFormat) -> anyhow::Result<String>
where
    I: IntoIterator,
    <I as IntoIterator>::Item: Tabled + Serialize + 'static,
{
    let iter = items.into_iter();
    match fmt {
        OutputFormat::Table => {
            let mut table = Table::new(iter);
            let n = table.count_rows() - 1;
            Ok(format!("{}\n{} records found", table.styled(), n,))
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(vec![]);
            iter.map(|item| writer.serialize(item))
                .collect::<Result<Vec<_>, _>>()?;
            writer.flush()?;
            String::from_utf8(writer.into_inner()?).map_err(|e| e.into())
        }
        OutputFormat::Json => {
            serde_json::to_string(&iter.collect::<Vec<_>>()).map_err(|e| e.into())
        }
        OutputFormat::Yaml => {
            serde_yaml::to_string(&iter.collect::<Vec<_>>()).map_err(|e| e.into())
        }
    }
}

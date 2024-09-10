use anyhow::anyhow;
use clap::ValueEnum;
use serde::Serialize;
use tabled::{Table, Tabled};
use thiserror::Error;

use crate::table::SeedctlTable;

pub mod rows;

#[derive(ValueEnum, Clone, Debug, PartialEq)]
pub enum OutputFormat {
    Table,
    Csv,
    Json,
    Yaml,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unable to create row")]
    UnableToCreateRow(#[from] libseed::Error),
}

pub fn format_one<T>(item: T, fmt: OutputFormat) -> anyhow::Result<String>
where
    T: Tabled + Serialize + 'static,
{
    match fmt {
        OutputFormat::Table => {
            let tbuilder = Table::builder(vec![item]).index().column(0).transpose();
            Ok(format!("{}n", tbuilder.build().styled()))
        }
        OutputFormat::Csv => Err(anyhow!("CSV format is not valid for single items")),
        OutputFormat::Json => serde_json::to_string(&item).map_err(|e| e.into()),
        OutputFormat::Yaml => serde_yaml::to_string(&item).map_err(|e| e.into()),
    }
}

pub fn format_seq<T>(mut items: Vec<T>, fmt: OutputFormat) -> anyhow::Result<String>
where
    T: Tabled + Serialize + 'static,
{
    match fmt {
        OutputFormat::Table => {
            let n = items.len();
            Ok(format!(
                "{}\n{} records found",
                Table::new(items).styled(),
                n
            ))
        }
        OutputFormat::Csv => {
            let mut writer = csv::Writer::from_writer(vec![]);
            items
                .drain(..)
                .map(|item| writer.serialize(item))
                .collect::<Result<Vec<_>, _>>()?;
            writer.flush()?;
            String::from_utf8(writer.into_inner()?).map_err(|e| e.into())
        }
        OutputFormat::Json => serde_json::to_string(&items).map_err(|e| e.into()),
        OutputFormat::Yaml => serde_yaml::to_string(&items).map_err(|e| e.into()),
    }
}

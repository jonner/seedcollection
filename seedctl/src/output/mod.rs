use clap::ValueEnum;
use csv::CsvFormatter;
use json::JsonFormatter;
use libseed::sample::Sample;
use serde::Serialize;
use table::TableFormatter;
use tabled::Tabled;
use thiserror::Error;

#[derive(ValueEnum, Clone, Debug)]
pub enum OutputFormat {
    Table,
    Csv,
    Json,
}

mod csv;
mod json;
mod table;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unable to create row")]
    UnableToCreateRow(#[from] libseed::Error),
}

pub trait Formatter {
    fn format_samples(&self, samples: Vec<Sample>) -> Result<String, anyhow::Error>;
}

pub fn formatter<T>(format: OutputFormat) -> Box<dyn Formatter>
where
    T: TryFrom<Sample> + Tabled + Serialize + 'static,
    <T as TryFrom<Sample>>::Error: Into<Error>,
{
    match format {
        OutputFormat::Table => TableFormatter::<T>::new(),
        OutputFormat::Csv => CsvFormatter::<T>::new(),
        OutputFormat::Json => JsonFormatter::<T>::new(),
    }
}

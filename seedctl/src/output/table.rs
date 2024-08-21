use crate::{
    output::{self, Formatter},
    table::SeedctlTable,
};
use libseed::sample::Sample;
use std::marker::PhantomData;
use tabled::{Table, Tabled};

pub struct TableFormatter<T>
where
    T: TryFrom<Sample> + Tabled + 'static,
    <T as TryFrom<Sample>>::Error: Into<output::Error>,
{
    phantom: PhantomData<T>,
}

impl<T> TableFormatter<T>
where
    T: TryFrom<Sample> + Tabled,
    <T as TryFrom<Sample>>::Error: Into<output::Error>,
{
    pub fn new() -> Box<dyn Formatter> {
        Box::new(TableFormatter::<T> {
            phantom: PhantomData,
        })
    }
}

impl<T> Formatter for TableFormatter<T>
where
    T: TryFrom<Sample> + Tabled,
    <T as TryFrom<Sample>>::Error: Into<output::Error>,
{
    fn format_samples(&self, mut samples: Vec<Sample>) -> Result<String, anyhow::Error> {
        let n = samples.len();
        let rows = samples
            .drain(..)
            .map(|sample| T::try_from(sample))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.into())?;
        let mut table = Table::new(rows);
        Ok(format!("{}\n{} records found", table.styled(), n))
    }
}

use std::io::stdout;
use std::marker::PhantomData;

use crate::output::Error;
use crate::output::Formatter;
use libseed::sample::Sample;
use serde::Serialize;

pub struct CsvFormatter<T>
where
    T: TryFrom<Sample> + Serialize,
    <T as TryFrom<Sample>>::Error: Into<Error>,
{
    phantom: PhantomData<T>,
}

impl<T> CsvFormatter<T>
where
    T: TryFrom<Sample> + Serialize + 'static,
    <T as TryFrom<Sample>>::Error: Into<Error>,
{
    pub fn new() -> Box<dyn Formatter> {
        Box::new(CsvFormatter::<T> {
            phantom: PhantomData,
        })
    }
}

impl<T> Formatter for CsvFormatter<T>
where
    T: TryFrom<Sample> + Serialize,
    <T as TryFrom<Sample>>::Error: Into<Error>,
{
    fn print_samples(&self, mut samples: Vec<Sample>) -> Result<(), anyhow::Error> {
        let mut rows = samples
            .drain(..)
            .map(|sample| T::try_from(sample))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.into())?;
        let mut writer = csv::Writer::from_writer(stdout());
        rows.drain(..)
            .map(|row| writer.serialize(row))
            .collect::<Result<Vec<_>, _>>()?;
        writer.flush()?;
        Ok(())
    }
}

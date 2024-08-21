use libseed::sample::Sample;
use tabled::{Table, Tabled};

use crate::output;
use crate::table::SeedctlTable;

pub fn list_samples<T>(mut samples: Vec<Sample>) -> Result<(), anyhow::Error>
where
    T: TryFrom<Sample> + Tabled,
    <T as TryFrom<Sample>>::Error: Into<output::Error>,
{
    let rows = samples
        .drain(..)
        .map(|sample| T::try_from(sample))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.into())?;
    let mut table = Table::new(rows);
    println!("{}\n", table.styled());
    println!("{} records found", samples.len());
    Ok(())
}

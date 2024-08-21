use std::io::stdout;

use crate::output;
use libseed::sample::Sample;
use serde::Serialize;

pub fn list_samples<T>(mut samples: Vec<Sample>) -> Result<(), anyhow::Error>
where
    T: TryFrom<Sample> + Serialize,
    <T as TryFrom<Sample>>::Error: Into<output::Error>,
{
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

use crate::output;
use libseed::sample::Sample;
use serde::Serialize;

pub fn list_samples<T>(mut samples: Vec<Sample>) -> Result<(), anyhow::Error>
where
    T: TryFrom<Sample> + Serialize,
    <T as TryFrom<Sample>>::Error: Into<output::Error>,
{
    let rows = samples
        .drain(..)
        .map(|sample| T::try_from(sample))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.into())?;
    let out = serde_json::to_string(&rows)?;
    println!("{}", out);
    Ok(())
}

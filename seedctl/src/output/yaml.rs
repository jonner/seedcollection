use std::marker::PhantomData;

use crate::output::{self, Formatter};
use libseed::sample::Sample;
use serde::Serialize;

pub struct YamlFormatter<T>
where
    T: TryFrom<Sample> + Serialize,
    <T as TryFrom<Sample>>::Error: Into<output::Error>,
{
    phantom: PhantomData<T>,
}

impl<T> YamlFormatter<T>
where
    T: TryFrom<Sample> + Serialize + 'static,
    <T as TryFrom<Sample>>::Error: Into<output::Error>,
{
    pub fn new() -> Box<dyn Formatter> {
        Box::new(YamlFormatter::<T> {
            phantom: PhantomData,
        })
    }
}

impl<T> Formatter for YamlFormatter<T>
where
    T: TryFrom<Sample> + Serialize,
    <T as TryFrom<Sample>>::Error: Into<output::Error>,
{
    fn format_samples(&self, mut samples: Vec<Sample>) -> Result<String, anyhow::Error> {
        let rows = samples
            .drain(..)
            .map(|sample| T::try_from(sample))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.into())?;
        serde_yaml::to_string(&rows).map_err(|e| e.into())
    }
}

use serde::{Deserialize, Deserializer};
use std::str::FromStr;

pub mod collection;
pub mod error;
pub mod filter;
pub mod loadable;
pub mod location;
pub mod note;
pub mod sample;
pub mod taxonomy;
pub mod user;

pub use error::Error;
pub use error::Result;

pub fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s)
            .map_err(serde::de::Error::custom)
            .map(Some),
    }
}

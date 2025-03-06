//! This is a library that provides objects and functionality to help you manage a native seed
//! collection and keep track of everything inside of a database.
use serde::{Deserialize, Deserializer};
use std::str::FromStr;

pub mod database;
mod error;
pub mod loadable;
pub mod project;
pub mod query;
pub mod sample;
pub mod source;
pub mod taxonomy;
pub mod user;

pub use error::Error;
pub use error::Result;

/// A utility function to deserialize an Optional value as None when it is an
/// empty string
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

use thiserror::Error;

mod csv;
mod json;
mod table;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Unable to create row")]
    UnableToCreateRow(#[from] libseed::Error),
}

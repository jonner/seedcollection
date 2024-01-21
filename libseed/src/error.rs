//! Objects related to reporting errors from this library

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error("Invalid data")]
    InvalidData(String),
    #[error("Authentication error")]
    Authentication(#[from] password_hash::Error),
    #[error("No permission for action")]
    NotAllowed(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

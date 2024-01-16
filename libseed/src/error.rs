#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error("Invalid data")]
    InvalidData(String),
    #[error("Authentication error")]
    Authentication(#[from] password_hash::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

//! Objects related to reporting errors from this library

#[derive(thiserror::Error, Debug)]
pub enum Error {
    // authentication-related errors
    #[error("authentication error: couldn't hash password")]
    AuthHashFailure(#[from] password_hash::Error),

    #[error("invalid username: too short")]
    AuthInvalidUsernameTooShort,

    #[error("invalid username: first character is invalid")]
    AuthInvalidUsernameFirstCharacter,

    #[error("invalid username: contains invalid characters")]
    AuthInvalidUsernameInvalidCharacters(String),

    #[error("The user could not be found")]
    AuthUserNotFound,

    #[error("invalid operation")]
    InvalidOperation(String),

    #[error("can't update the object, no id was specified")]
    InvalidOperationObjectNotFound,

    #[error("can't insert the object, it already exists in the database with id = {}", .0)]
    InvalidOperationObjectAlreadyExists(i64),

    #[error("invalid state: the object is not loaded")]
    InvalidStateNotLoaded,

    #[error("Invalid state: the object has an unspecified attribute '{}'", .0)]
    InvalidStateMissingAttribute(String),

    #[error("Database error: unspecified")]
    DatabaseUnspecified(#[source] sqlx::Error),

    #[error("Database error: row not found")]
    DatabaseRowNotFound(#[source] sqlx::Error),
}

impl std::convert::From<sqlx::Error> for Error {
    fn from(value: sqlx::Error) -> Self {
        match value {
            sqlx::Error::RowNotFound => Self::DatabaseRowNotFound(value),
            _ => Self::DatabaseUnspecified(value),
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

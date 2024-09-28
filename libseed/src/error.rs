//! Objects related to reporting errors from this library

/// A list of error types that can occur within this library
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

    #[error("invalid operation: {0}")]
    InvalidOperation(String),

    #[error("can't update the object, no id was specified")]
    InvalidUpdateObjectNotFound,

    #[error("can't insert the object, it already exists in the database with id = {}", .0)]
    InvalidInsertObjectAlreadyExists(i64),

    #[error("invalid state: the object is not loaded")]
    InvalidStateNotLoaded,

    #[error("Invalid state: the object has an unspecified attribute '{}'", .0)]
    InvalidStateMissingAttribute(String),

    #[error(transparent)]
    DatabaseError(#[from] sqlx::Error),
}

/// A convenience type alias for a [Result] with [Error](self::Error) as its error type
pub type Result<T, E = Error> = std::result::Result<T, E>;

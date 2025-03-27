//! Objects related to reporting errors from this library

#[derive(Debug, thiserror::Error)]
pub enum VerificationError {
    #[error("Verification code expired")]
    Expired,
    #[error("Verification code not found")]
    KeyNotFound,
    #[error("Verification code already verified")]
    AlreadyVerified,
    #[error("Multiple verification codes found for same key")]
    MultipleKeysFound,
    #[error(transparent)]
    InternalError(#[from] sqlx::Error),
}

/// A list of error types that can occur within this library
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
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

    #[error(transparent)]
    DatabaseMigrationError(#[from] sqlx::migrate::MigrateError),

    #[error("Database upgrade failed: {0}")]
    DatabaseUpgrade(String),

    #[error(transparent)]
    UserVerification(#[from] VerificationError),
}

/// A convenience type alias for a [Result] with [Error] as its error type
pub type Result<T, E = Error> = std::result::Result<T, E>;

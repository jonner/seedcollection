use axum::{
    extract::rejection::{FormRejection, QueryRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use tracing::warn;

use crate::{auth::SqliteAuthBackend, email};

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
    #[error("You are not authorized to perform this action: {0}")]
    Unauthorized(String),
    #[error(transparent)]
    Libseed(#[from] libseed::Error),
    #[error("Resource Not Found: {0}")]
    NotFound(String),
    #[error("The provided query string was rejected: {0}")]
    QueryExtractorRejection(#[source] QueryRejection),
    #[error("The submitted form was rejected: {0}")]
    FormExtractorRejection(#[source] FormRejection),
    #[error("The environment is not set up correctly: {0}")]
    Environment(String),
    #[error("New user registration is currently disabled")]
    UserRegistrationDisabled,
    #[error("Required parameter '{0}' is missing")]
    RequiredParameterMissing(String),
    #[error("Operation failed: {0}")]
    OperationFailed(String),
    #[error(transparent)]
    Auth(#[from] axum_login::Error<SqliteAuthBackend>),
    #[error("User {0} not found")]
    UserNotFound(String),
    #[error("User {0} not found")]
    RegistrationValidation(#[from] crate::html::auth::RegistrationValidationError),
    #[error(transparent)]
    MailService(#[from] email::Error),
}

impl Error {
    pub(crate) fn to_client_status(&self) -> (StatusCode, String) {
        match self {
            Error::Libseed(libseed::Error::DatabaseError(_)) | Error::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            ),
            Error::Unauthorized(message) => (StatusCode::UNAUTHORIZED, message.clone()),
            Error::NotFound(message) => (StatusCode::NOT_FOUND, message.clone()),
            Error::QueryExtractorRejection(rejection) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("The query string could not be processed: {rejection}"),
            ),
            Error::FormExtractorRejection(rejection) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("The form could not be processed: {rejection}"),
            ),
            // FIXME: handle more specific libseed errors?
            Error::Libseed(_) | Error::Other(_) | Error::Environment(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal error".to_string(),
            ),
            Error::UserRegistrationDisabled => (StatusCode::UNAUTHORIZED, self.to_string()),
            Error::RequiredParameterMissing(param) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("Missing parameter '{param}'"),
            ),
            Error::OperationFailed(message) => (StatusCode::UNPROCESSABLE_ENTITY, message.clone()),
            Error::Auth(_) | Error::UserNotFound(_) => (
                StatusCode::UNAUTHORIZED,
                "Failed to log in. Please double-check username and password and try again."
                    .to_string(),
            ),
            Error::RegistrationValidation(r) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                format!(
                    "Could not register user:\n{}",
                    r.issues
                        .iter()
                        .map(|s| format!(" - {s}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            ),
            Error::MailService(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Mail Service error: {error}"),
            ),
        }
    }
}

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        warn!("Got error for response: {self:?}");
        // placeholder, will get refined in the response mapper
        let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
        // insert the error into the response so that we can log it in the response mapper.
        response.extensions_mut().insert(Arc::new(self));
        response
    }
}

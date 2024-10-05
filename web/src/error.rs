use std::sync::Arc;

use axum::{
    extract::rejection::QueryRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::warn;
#[derive(thiserror::Error, Debug)]
pub enum Error {
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
    UnprocessableEntityQueryRejection(#[source] QueryRejection),
    #[error("The environment is not set up correctly: {0}")]
    Environment(String),
}

impl Error {
    pub fn to_client_status(&self) -> (StatusCode, String) {
        match self {
            Error::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            ),
            Error::Other(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unknown error".to_string(),
            ),
            // FIXME: make this more specific
            Error::Libseed(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Library error".to_string()),
            Error::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "Not authorized".to_string()),
            Error::NotFound(_) => (StatusCode::NOT_FOUND, "Page not found".to_string()),
            Error::UnprocessableEntityQueryRejection(_) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "The query string was not in the expected format. The request could not be processed.".to_string(),
            ),
            Error::Environment(_message) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal error".to_string())
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

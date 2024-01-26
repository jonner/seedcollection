use std::sync::Arc;

use axum::{
    extract::rejection::QueryRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::warn;
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error("Other error")]
    Other(#[from] anyhow::Error),
    #[error("Redirect to another url")]
    Unauthorized(String),
    #[error("Library error")]
    Libseed(#[from] libseed::Error),
    #[error("Not Found")]
    NotFound(String),
    #[error("Bad request")]
    BadRequestQueryRejection(#[source] QueryRejection),
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
            Error::Libseed(_) => (StatusCode::INTERNAL_SERVER_ERROR, format!("Library error")),
            Error::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "Not authorized".to_string()),
            Error::NotFound(_) => (StatusCode::NOT_FOUND, "Page not found".to_string()),
            Error::BadRequestQueryRejection(_) => (
                StatusCode::BAD_REQUEST,
                "Query string was malformed. The request could not be handled.".to_string(),
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

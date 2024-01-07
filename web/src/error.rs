use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::warn;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Authentication error")]
    PasswordHashFailure(#[from] password_hash::errors::Error),
    #[error("Database error")]
    Database(#[from] sqlx::Error),
    #[error("Other error")]
    Other(#[from] anyhow::Error),
    #[error("Redirect to another url")]
    Unauthorized(String),
    #[error("Not Found")]
    NotFound(String),
}

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Self::NotFound(s) => (
                StatusCode::NOT_FOUND,
                format!("The resource you requested was not found: {}", s),
            )
                .into_response(),
            Self::Unauthorized(s) => (
                StatusCode::UNAUTHORIZED,
                format!("You don't have permission to see this page: {}", s),
            )
                .into_response(),
            _ => {
                warn!("An error occurred: {:?}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Something went wrong: {}", self),
                )
                    .into_response()
            }
        }
    }
}

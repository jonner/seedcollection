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
    DatabaseError(#[from] sqlx::Error),
    #[error("Other error")]
    OtherError(#[from] anyhow::Error),
    #[error("Redirect to another url")]
    Unauthorized(String),
}

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
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

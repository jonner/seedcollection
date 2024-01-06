use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Authentication error")]
    PasswordHashFailure(#[from] password_hash::errors::Error),
    #[error("Database error")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Other error")]
    OtherError(#[from] anyhow::Error),
}

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self),
        )
            .into_response()
    }
}

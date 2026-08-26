use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("resource not found")]
    NotFound,

    #[error("{0}")]
    BadRequest(String),

    #[error("could not validate credentials")]
    Unauthorized,

    #[error("you do not have permission to perform this action")]
    Forbidden,

    #[error("{0}")]
    Conflict(String),

    #[error("too many requests")]
    TooManyRequests,

    #[error(transparent)]
    Database(#[from] sqlx::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }

    fn parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m.clone()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", self.to_string()),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m.clone()),
            AppError::TooManyRequests => {
                (StatusCode::TOO_MANY_REQUESTS, "rate_limited", self.to_string())
            }
            AppError::Database(sqlx::Error::RowNotFound) => {
                (StatusCode::NOT_FOUND, "not_found", "resource not found".into())
            }
            AppError::Database(_) | AppError::Other(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "internal server error".into(),
            ),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error, message) = self.parts();
        if status.is_server_error() {
            tracing::error!(%self, "request failed");
        }
        (status, Json(ErrorBody { error, message })).into_response()
    }
}

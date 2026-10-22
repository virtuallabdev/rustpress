use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("Authentication required")]
    Unauthorized,
    #[error("Permission denied")]
    Forbidden,
    #[error("Resource not found")]
    NotFound,
    #[error("Version conflict: reload before saving")]
    Conflict,
    #[error("Too many requests")]
    RateLimited,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Internal operation failed: {0}")]
    Internal(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::Invalid(message) => (StatusCode::BAD_REQUEST, message.clone()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            Self::Forbidden => (StatusCode::FORBIDDEN, self.to_string()),
            Self::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            Self::Conflict => (StatusCode::CONFLICT, self.to_string()),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            Self::Db(sqlx::Error::Database(db)) if db.is_unique_violation() => (
                StatusCode::CONFLICT,
                "A resource with this identifier already exists".into(),
            ),
            _ => {
                tracing::error!(error = %self, "Request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal server error".into(),
                )
            }
        };
        (
            status,
            Json(json!({"error": {"code": status.as_u16(), "message": message}})),
        )
            .into_response()
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

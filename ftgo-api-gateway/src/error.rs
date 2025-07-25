use axum::{http::StatusCode, response::Json};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Authentication failed")]
    AuthenticationFailed,
    #[error("Invalid token")]
    InvalidToken,
    #[error("Access forbidden")]
    Forbidden,
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_message) = match &self {
            ApiError::AuthenticationFailed => (
                StatusCode::UNAUTHORIZED,
                "Authentication failed".to_string(),
            ),
            ApiError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token".to_string()),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "Access forbidden".to_string()),
            ApiError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg.clone()),
        };

        let body = Json(json!({
            "error": error_message
        }));

        (status, body).into_response()
    }
}

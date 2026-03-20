// Error types for axum-apcore.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Unified error type for axum-apcore operations.
#[derive(Debug, thiserror::Error)]
pub enum AxumApcoreError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Scanner error: {0}")]
    Scanner(String),

    #[error("Registration error: {0}")]
    Registration(String),

    #[error("Context extraction error: {0}")]
    Context(String),

    #[error("Module execution error: {0}")]
    Execution(#[from] apcore::ModuleError),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl IntoResponse for AxumApcoreError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AxumApcoreError::Config(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AxumApcoreError::Scanner(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AxumApcoreError::Registration(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            AxumApcoreError::Context(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            AxumApcoreError::Execution(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.message.clone()),
            AxumApcoreError::Regex(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            AxumApcoreError::Json(e) => (StatusCode::BAD_REQUEST, e.to_string()),
        };

        let body = serde_json::json!({
            "error": message,
        });
        (status, axum::Json(body)).into_response()
    }
}

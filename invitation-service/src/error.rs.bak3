use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use log::{error, warn};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Invitation expired")]
    InvitationExpired,

    #[error("Internal server error: {0}")]
    InternalServerError(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Bad gateway: {0}")]
    #[allow(dead_code)]
    BadGateway(String),
}

impl AppError {
    #[allow(dead_code)]
    pub fn not_found(msg: String) -> Self {
        warn!("Not found error: {}", msg);
        Self::NotFound(msg)
    }

    #[allow(dead_code)]
    pub fn unauthorized(msg: String) -> Self {
        warn!("Unauthorized error: {}", msg);
        Self::Unauthorized(msg)
    }

    #[allow(dead_code)]
    pub fn bad_request(msg: String) -> Self {
        warn!("Bad request error: {}", msg);
        Self::BadRequest(msg)
    }

    #[allow(dead_code)]
    pub fn invitation_expired() -> Self {
        warn!("Invitation expired");
        Self::InvitationExpired
    }

    #[allow(dead_code)]
    pub fn internal_server_error(msg: String) -> Self {
        error!("Internal server error: {}", msg);
        Self::InternalServerError(msg)
    }

    #[allow(dead_code)]
    pub fn forbidden(msg: String) -> Self {
        warn!("Forbidden: {}", msg);
        Self::Forbidden(msg)
    }

    #[allow(dead_code)]
    pub fn bad_gateway(msg: String) -> Self {
        warn!("Bad gateway error: {}", msg);
        Self::BadGateway(msg)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::InvitationExpired => (StatusCode::GONE, "Invitation has expired".to_string()),
            AppError::InternalServerError(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::SerializationError(err) => {
                warn!("Serialization error: {}", err);
                (StatusCode::BAD_REQUEST, err.to_string())
            }
            AppError::BadGateway(msg) => (StatusCode::BAD_GATEWAY, msg),
        };

        let body = Json(json!({ "error": error_message }));
        (status, body).into_response()
    }
}

// Helper function to map DynamoDB errors to our application errors
pub fn map_dynamo_error(operation: &str, err: impl std::fmt::Display) -> AppError {
    AppError::InternalServerError(format!("DynamoDB {} error: {}", operation, err))
}

// Add conversion from shared StoreError to AppError
impl From<lockbox_shared::error::StoreError> for AppError {
    fn from(err: lockbox_shared::error::StoreError) -> Self {
        match err {
            lockbox_shared::error::StoreError::NotFound(msg) => AppError::NotFound(msg),
            lockbox_shared::error::StoreError::ValidationError(msg) => AppError::BadRequest(msg),
            lockbox_shared::error::StoreError::InternalError(msg) => {
                error!("Store internal error: {}", msg);
                AppError::InternalServerError(msg)
            }
            lockbox_shared::error::StoreError::InvitationExpired => AppError::InvitationExpired,
            lockbox_shared::error::StoreError::AuthError(msg) => AppError::Unauthorized(msg),
            lockbox_shared::error::StoreError::VersionConflict(msg) => {
                AppError::BadRequest(format!("Concurrent modification detected: {}", msg))
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

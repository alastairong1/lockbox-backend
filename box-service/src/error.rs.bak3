use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use log::{error, info, warn};
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal server error: {0}")]
    InternalServerError(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    // Add a specific variant for expired invitations with status 422
    #[error("Invitation expired: {0}")]
    InvitationExpired(String),
}

// Add back compatibility methods
impl AppError {
    pub fn unauthorized(msg: String) -> Self {
        warn!("Unauthorized error: {}", msg);
        AppError::Unauthorized(msg)
    }

    pub fn not_found(msg: String) -> Self {
        warn!("Not found error: {}", msg);
        AppError::NotFound(msg)
    }

    pub fn bad_request(msg: String) -> Self {
        warn!("Bad request error: {}", msg);
        AppError::BadRequest(msg)
    }

    pub fn internal_server_error(msg: String) -> Self {
        error!("Internal server error: {}", msg);
        AppError::InternalServerError(msg)
    }

    #[allow(dead_code)]
    pub fn internal_error<T: std::fmt::Display>(error: T) -> Self {
        AppError::InternalServerError(error.to_string())
    }

    #[allow(dead_code)]
    pub fn not_found_error(message: String) -> Self {
        AppError::NotFound(message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AppError::Unauthorized(msg) => {
                warn!("Unauthorized error: {}", msg);
                (StatusCode::UNAUTHORIZED, msg.clone())
            }
            AppError::NotFound(msg) => {
                warn!("Not found error: {}", msg);
                (StatusCode::NOT_FOUND, msg.clone())
            }
            AppError::BadRequest(msg) => {
                warn!("Bad request error: {}", msg);
                (StatusCode::BAD_REQUEST, msg.clone())
            }
            AppError::InternalServerError(msg) => {
                error!("Internal server error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, msg.clone())
            }
            AppError::SerializationError(err) => {
                warn!("Serialization error: {}", err);
                (StatusCode::BAD_REQUEST, err.to_string())
            }
            AppError::InvitationExpired(msg) => {
                warn!("Invitation expired: {}", msg);
                (StatusCode::UNPROCESSABLE_ENTITY, msg.clone())
            }
        };

        let body = Json(json!({ "error": error_message }));
        info!(
            "Responding with error: status={}, message={:?}",
            status, body
        );
        (status, body).into_response()
    }
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
            lockbox_shared::error::StoreError::InvitationExpired => {
                // Map to the specific 422 error variant
                AppError::InvitationExpired("Invitation has expired".into())
            }
            lockbox_shared::error::StoreError::AuthError(msg) => AppError::Unauthorized(msg),
            lockbox_shared::error::StoreError::VersionConflict(msg) => {
                warn!("Concurrent modification detected: {}", msg);
                AppError::BadRequest(
                    format!("Concurrent modification detected, please retry: {}", msg).into(),
                )
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

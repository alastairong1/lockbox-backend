// This file exists primarily to provide a Result type for trait interfaces
// Each service should implement its own error handling

// Define a simple error type that services can map from
#[derive(Debug)]
pub enum StoreError {
    NotFound(String),
    InternalError(String),
    ValidationError(String),
    InvitationExpired,
    AuthError(String),
    VersionConflict(String),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::NotFound(msg) => write!(f, "Not found: {}", msg),
            StoreError::InternalError(msg) => write!(f, "Internal error: {}", msg),
            StoreError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            StoreError::InvitationExpired => write!(f, "Invitation expired"),
            StoreError::AuthError(msg) => write!(f, "Authentication error: {}", msg),
            StoreError::VersionConflict(msg) => write!(f, "Version conflict: {}", msg),
        }
    }
}

impl std::error::Error for StoreError {}

// Define a result type for store interfaces
pub type Result<T> = std::result::Result<T, StoreError>;

// Useful conversions
impl From<serde_dynamo::Error> for StoreError {
    fn from(err: serde_dynamo::Error) -> Self {
        StoreError::InternalError(format!("DynamoDB serialization error: {}", err))
    }
}

impl From<serde_json::Error> for StoreError {
    fn from(err: serde_json::Error) -> Self {
        StoreError::InternalError(format!("JSON serialization error: {}", err))
    }
}

// Basic error mapping functions that store implementations may use
pub fn map_dynamo_error(operation: &str, err: impl std::fmt::Display) -> StoreError {
    StoreError::InternalError(format!("DynamoDB {} error: {}", operation, err))
}

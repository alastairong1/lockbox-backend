use thiserror::Error;

#[derive(Error, Debug)]
pub enum NotificationError {
    #[error("Failed to look up push tokens: {0}")]
    TokenLookupFailed(String),

    #[error("Failed to send push notification: {0}")]
    SendFailed(String),
}

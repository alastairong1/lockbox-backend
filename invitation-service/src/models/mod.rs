use serde::{Deserialize, Serialize};
use lockbox_shared::models::MessageResponse;

// Request DTOs
#[derive(Deserialize, Debug)]
pub struct CreateInvitationRequest {
    #[serde(rename = "invitedName")]
    pub invited_name: String,
    #[serde(rename = "boxId")]
    pub box_id: String,
}

#[derive(Deserialize, Debug)]
pub struct ConnectToUserRequest {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "inviteCode")]
    pub invite_code: String,
}

// Use shared MessageResponse from lockbox_shared

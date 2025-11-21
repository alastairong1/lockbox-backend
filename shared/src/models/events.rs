use serde::{Deserialize, Serialize};

/// Event for box invitations
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct InvitationEvent {
    pub event_type: String,
    pub invitation_id: String,
    pub box_id: String,
    pub user_id: Option<String>,
    pub invite_code: String,
    pub invited_name: Option<String>,
    #[serde(default)]
    pub is_lead_guardian: bool,
    pub timestamp: String,
}

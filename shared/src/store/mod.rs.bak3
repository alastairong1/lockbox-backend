use async_trait::async_trait;

use crate::error::Result;
use crate::models::{BoxRecord, GuardianStatus, Invitation};

// Expose the DynamoDB store module
pub mod dynamo;

/// InvitationStore trait defining the interface for invitation storage implementations
#[async_trait]
pub trait InvitationStore: Send + Sync + 'static {
    /// Creates a new invitation
    async fn create_invitation(&self, invitation: Invitation) -> Result<Invitation>;

    /// Gets an invitation by ID
    async fn get_invitation(&self, id: &str) -> Result<Invitation>;

    /// Gets an invitation by invite code
    async fn get_invitation_by_code(&self, invite_code: &str) -> Result<Invitation>;

    /// Updates an invitation
    async fn update_invitation(&self, invitation: Invitation) -> Result<Invitation>;

    /// Deletes an invitation
    async fn delete_invitation(&self, id: &str) -> Result<()>;

    /// Gets all invitations for a box
    async fn get_invitations_by_box_id(&self, box_id: &str) -> Result<Vec<Invitation>>;

    /// Gets all invitations created by a specific user
    async fn get_invitations_by_creator_id(&self, creator_id: &str) -> Result<Vec<Invitation>>;
}

/// BoxStore trait defining the interface for box storage implementations
#[async_trait]
pub trait BoxStore: Send + Sync + 'static {
    /// Creates a new box
    async fn create_box(&self, box_record: BoxRecord) -> Result<BoxRecord>;

    /// Gets a box by ID
    async fn get_box(&self, id: &str) -> Result<BoxRecord>;

    /// Gets all boxes owned by a user
    async fn get_boxes_by_owner(&self, owner_id: &str) -> Result<Vec<BoxRecord>>;

    /// Gets all boxes where the given user is a guardian (with status not rejected)
    async fn get_boxes_by_guardian_id(&self, guardian_id: &str) -> Result<Vec<BoxRecord>>;

    /// Updates a box
    async fn update_box(&self, box_record: BoxRecord) -> Result<BoxRecord>;

    /// Deletes a box
    async fn delete_box(&self, id: &str) -> Result<()>;
}

// Box store utility functions
pub fn convert_to_guardian_box(
    box_rec: &BoxRecord,
    user_id: &str,
) -> Option<crate::models::GuardianBox> {
    if let Some(guardian) = box_rec
        .guardians
        .iter()
        .find(|g| g.id == user_id && g.status != GuardianStatus::Rejected)
    {
        let pending = matches!(
            guardian.status,
            GuardianStatus::Invited | GuardianStatus::Viewed
        );
        let is_lead = guardian.lead_guardian;
        Some(crate::models::GuardianBox {
            id: box_rec.id.clone(),
            name: box_rec.name.clone(),
            description: box_rec.description.clone(),
            is_locked: box_rec.is_locked,
            created_at: box_rec.created_at.clone(),
            updated_at: box_rec.updated_at.clone(),
            owner_id: box_rec.owner_id.clone(),
            owner_name: box_rec.owner_name.clone(),
            unlock_instructions: box_rec.unlock_instructions.clone(),
            unlock_request: box_rec.unlock_request.clone(),
            pending_guardian_approval: Some(pending),
            guardians_count: box_rec.guardians.len(),
            is_lead_guardian: is_lead,
            documents: box_rec.documents.clone(),
            guardians: box_rec.guardians.clone(),
        })
    } else {
        None
    }
}

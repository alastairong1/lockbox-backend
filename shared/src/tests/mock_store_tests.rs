use crate::models::BoxRecord;
use crate::store::BoxStore;
use std::sync::Arc;
use uuid::Uuid;

use crate::models::Invitation;
use crate::store::InvitationStore;
use crate::test_utils::mock_box_store::MockBoxStore;
use crate::test_utils::mock_invitation_store::MockInvitationStore;

#[tokio::test]
async fn test_mock_box_store() {
    // Create a mock store
    let store = Arc::new(MockBoxStore::new());

    // Create a test box
    let box_id = Uuid::new_v4().to_string();
    let owner_id = "test_user";
    let now = crate::models::now_str();

    let test_box = BoxRecord {
        id: box_id.clone(),
        name: "Test Box".to_string(),
        description: "Test Description".to_string(),
        is_locked: false,
        locked_at: None,
        created_at: now.clone(),
        updated_at: now.clone(),
        owner_id: owner_id.to_string(),
        owner_name: Some("Test User".to_string()),
        documents: vec![],
        guardians: vec![],
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
        shard_threshold: None,
        shards_fetched: None,
        total_shards: None,
        shards_deleted_at: None,
    };

    // Store the box
    let result = store.create_box(test_box.clone()).await;
    assert!(result.is_ok());

    // Get the box
    let get_result = store.get_box(&box_id).await;
    assert!(get_result.is_ok());
    let retrieved_box = get_result.unwrap();
    assert_eq!(retrieved_box.id, box_id);
    assert_eq!(retrieved_box.name, "Test Box");

    // Get boxes by owner
    let owner_boxes = store.get_boxes_by_owner(owner_id).await.unwrap();
    assert_eq!(owner_boxes.len(), 1);
    assert_eq!(owner_boxes[0].id, box_id);

    // Update the box
    let mut updated_box = test_box.clone();
    updated_box.name = "Updated Box".to_string();
    let update_result = store.update_box(updated_box).await;
    assert!(update_result.is_ok());

    // Verify update
    let get_updated = store.get_box(&box_id).await.unwrap();
    assert_eq!(get_updated.name, "Updated Box");

    // Delete the box
    let delete_result = store.delete_box(&box_id).await;
    assert!(delete_result.is_ok());

    // Verify deletion
    let get_deleted = store.get_box(&box_id).await;
    assert!(get_deleted.is_err());
}

#[tokio::test]
async fn test_mock_invitation_store() {
    // Create a mock store
    let store = Arc::new(MockInvitationStore::new());

    // Create a test invitation
    let invitation_id = Uuid::new_v4().to_string();
    let box_id = Uuid::new_v4().to_string();
    let creator_id = "test_creator";
    let invite_code = "testcode123";
    let now = crate::models::now_str();

    let test_invitation = Invitation {
        id: invitation_id.clone(),
        invite_code: invite_code.to_string(),
        invited_name: "Test Invitee".to_string(),
        box_id: box_id.clone(),
        created_at: now.clone(),
        expires_at: now.clone(), // In a real scenario, this would be future time
        opened: false,
        linked_user_id: None,
        creator_id: creator_id.to_string(),
        is_lead_guardian: false,
    };

    // Store the invitation
    let result = store.create_invitation(test_invitation.clone()).await;
    assert!(result.is_ok());

    // Get the invitation by ID
    let get_result = store.get_invitation(&invitation_id).await;
    assert!(get_result.is_ok());
    let retrieved_invitation = get_result.unwrap();
    assert_eq!(retrieved_invitation.id, invitation_id);
    assert_eq!(retrieved_invitation.invite_code, invite_code);

    // Get the invitation by code
    let get_by_code_result = store.get_invitation_by_code(invite_code).await;
    assert!(get_by_code_result.is_ok());
    let retrieved_by_code = get_by_code_result.unwrap();
    assert_eq!(retrieved_by_code.id, invitation_id);

    // Get invitations by box ID
    let box_invitations = store.get_invitations_by_box_id(&box_id).await.unwrap();
    assert_eq!(box_invitations.len(), 1);
    assert_eq!(box_invitations[0].id, invitation_id);

    // Get invitations by creator ID
    let creator_invitations = store
        .get_invitations_by_creator_id(creator_id)
        .await
        .unwrap();
    assert_eq!(creator_invitations.len(), 1);
    assert_eq!(creator_invitations[0].id, invitation_id);

    // Update the invitation
    let mut updated_invitation = test_invitation.clone();
    updated_invitation.opened = true;
    updated_invitation.linked_user_id = Some("test_user".to_string());
    let update_result = store.update_invitation(updated_invitation).await;
    assert!(update_result.is_ok());

    // Verify update
    let get_updated = store.get_invitation(&invitation_id).await.unwrap();
    assert_eq!(get_updated.opened, true);
    assert_eq!(get_updated.linked_user_id, Some("test_user".to_string()));

    // Delete the invitation
    let delete_result = store.delete_invitation(&invitation_id).await;
    assert!(delete_result.is_ok());

    // Verify deletion
    let get_deleted = store.get_invitation(&invitation_id).await;
    assert!(get_deleted.is_err());
}

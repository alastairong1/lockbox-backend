use aws_lambda_events::event::sns::{SnsEvent, SnsMessage, SnsRecord};
use chrono::Utc;
use lambda_runtime::LambdaEvent;
use std::collections::HashMap;
use std::sync::Arc;

use lockbox_shared::error::Result as StoreResult;
use lockbox_shared::models::events::InvitationEvent;
use lockbox_shared::models::GuardianStatus;
use lockbox_shared::store::dynamo::DynamoBoxStore;
use lockbox_shared::store::BoxStore;
use lockbox_shared::test_utils::dynamo_test_utils::{
    clear_dynamo_table, create_box_table, create_dynamo_client, use_dynamodb,
};
use lockbox_shared::test_utils::mock_box_store::MockBoxStore;
use lockbox_shared::test_utils::test_logging;

use crate::handler;

// Constants for DynamoDB tests
const TEST_TABLE_NAME: &str = "box-invitation-test-table";

#[derive(Clone)]
enum TestStore {
    Mock(Arc<MockBoxStore>),
    DynamoDB(Arc<DynamoBoxStore>),
}

impl TestStore {
    // Helper method to create a box in either store type
    async fn create_box(
        &self,
        box_record: lockbox_shared::models::BoxRecord,
    ) -> StoreResult<lockbox_shared::models::BoxRecord> {
        match self {
            TestStore::Mock(store) => store.create_box(box_record).await,
            TestStore::DynamoDB(store) => store.create_box(box_record).await,
        }
    }

    // Helper method to get a box from either store type
    async fn get_box(&self, box_id: &str) -> StoreResult<lockbox_shared::models::BoxRecord> {
        match self {
            TestStore::Mock(store) => store.get_box(box_id).await,
            TestStore::DynamoDB(store) => store.get_box(box_id).await,
        }
    }

    // Helper method to pass the right store to handler function
    async fn handle_event(
        &self,
        event: LambdaEvent<SnsEvent>,
    ) -> Result<(), lambda_runtime::Error> {
        match self {
            TestStore::Mock(store) => handler(event, store.clone()).await,
            TestStore::DynamoDB(store) => handler(event, store.clone()).await,
        }
    }
}

// Helper for setting up test store
async fn create_test_store() -> TestStore {
    // Initialize logging for tests
    test_logging::init_test_logging();

    if use_dynamodb() {
        // Set up DynamoDB store
        let client = create_dynamo_client().await;

        // Ensure the table is clean before the test
        let _ = clear_dynamo_table(&client, TEST_TABLE_NAME).await;

        // Create/clear the test table
        match create_box_table(&client, TEST_TABLE_NAME).await {
            Ok(_) => log::debug!("Test table created/exists successfully"),
            Err(e) => log::error!("Error setting up test table: {}", e),
        }

        // Create the store with the test table
        let store = Arc::new(DynamoBoxStore::with_client_and_table(
            client.clone(),
            TEST_TABLE_NAME.to_string(),
        ));

        TestStore::DynamoDB(store)
    } else {
        // Use mock store
        let store = Arc::new(MockBoxStore::new());
        TestStore::Mock(store)
    }
}

// Helper to create an SNS event for testing
fn create_test_sns_event(
    event_type: &str,
    invitation_id: &str,
    box_id: &str,
    user_id: &str,
) -> LambdaEvent<SnsEvent> {
    create_test_sns_event_with_name(event_type, invitation_id, box_id, user_id, None)
}

// Helper to create an SNS event with optional invited_name and is_lead_guardian for testing
fn create_test_sns_event_with_name(
    event_type: &str,
    invitation_id: &str,
    box_id: &str,
    user_id: &str,
    invited_name: Option<String>,
) -> LambdaEvent<SnsEvent> {
    create_test_sns_event_full(
        event_type,
        invitation_id,
        box_id,
        user_id,
        invited_name,
        false,
    )
}

// Full helper to create an SNS event with all optional fields for testing
fn create_test_sns_event_full(
    event_type: &str,
    invitation_id: &str,
    box_id: &str,
    user_id: &str,
    invited_name: Option<String>,
    is_lead_guardian: bool,
) -> LambdaEvent<SnsEvent> {
    // Create invitation event
    let invitation_event = InvitationEvent {
        event_type: event_type.to_string(),
        invitation_id: invitation_id.to_string(),
        box_id: box_id.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        user_id: if user_id.is_empty() {
            None
        } else {
            Some(user_id.to_string())
        },
        invite_code: "test-code".to_string(),
        invited_name,
        is_lead_guardian,
    };

    // Serialize to JSON
    let event_json = serde_json::to_string(&invitation_event).unwrap();

    // Create SNS message with correct field names
    let sns_message = SnsMessage {
        signature: "test-signature".to_string(),
        message_id: "test-message-id".to_string(),
        topic_arn: "arn:aws:sns:us-east-1:123456789012:invitation-events".to_string(),
        subject: Some("Invitation Event".to_string()),
        message: event_json,
        timestamp: Utc::now(),
        signature_version: "1".to_string(),
        signing_cert_url: "https://sns.us-east-1.amazonaws.com/cert.pem".to_string(),
        unsubscribe_url: "https://sns.us-east-1.amazonaws.com/unsubscribe".to_string(),
        message_attributes: HashMap::new(),
        sns_message_type: "Notification".to_string(),
    };

    // Create SNS record
    let sns_record = SnsRecord {
        event_version: "1.0".to_string(),
        event_subscription_arn:
            "arn:aws:sns:us-east-1:123456789012:invitation-events:subscription-id".to_string(),
        event_source: "aws:sns".to_string(),
        sns: sns_message,
    };

    // Create SNS event
    let sns_event = SnsEvent {
        records: vec![sns_record],
    };

    // Create Lambda event
    LambdaEvent {
        payload: sns_event,
        context: lambda_runtime::Context::default(),
    }
}

#[tokio::test]
async fn test_invitation_created_handler() {
    // Create test store
    let store = create_test_store().await;

    // Create test SNS event for invitation_created
    let box_id = "test_box_create_456";
    let invitation_id = "test_invitation_create_789";
    let invited_name = "Alice Smith";
    let event = create_test_sns_event_with_name(
        "invitation_created",
        invitation_id,
        box_id,
        "", // No user_id for invitation_created events
        Some(invited_name.to_string()),
    );

    // Create a test box record without any guardians initially
    let box_record = lockbox_shared::models::BoxRecord {
        id: box_id.to_string(),
        name: "Test Box".to_string(),
        description: "Test Description".to_string(),
        is_locked: false,
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        owner_id: "test_owner".to_string(),
        owner_name: Some("Test Owner".to_string()),
        documents: vec![],
        guardians: vec![], // Start with no guardians
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
    };

    // Add test box to store
    let _ = store.create_box(box_record).await.unwrap();

    // Call handler with the appropriate box store
    let result = store.handle_event(event).await;
    assert!(result.is_ok(), "Handler failed: {:?}", result.err());

    // Verify guardian was created correctly after invitation_created event
    let box_result = store.get_box(box_id).await;
    assert!(
        box_result.is_ok(),
        "Failed to retrieve box: {:?}",
        box_result.err()
    );

    // Get the box record and examine it
    let box_record = box_result.unwrap();

    // Verify a guardian was created
    assert_eq!(
        box_record.guardians.len(),
        1,
        "Exactly one guardian should be created"
    );

    // Find the guardian with the matching invitation_id
    let guardian = box_record
        .guardians
        .iter()
        .find(|g| g.invitation_id == invitation_id)
        .expect("Guardian with matching invitation_id should exist");

    // Verify guardian fields
    assert_eq!(
        guardian.id, "",
        "Guardian id should be empty until invitation is opened"
    );
    assert_eq!(
        guardian.name, invited_name,
        "Guardian name should match invited_name from event"
    );
    assert_eq!(
        guardian.status,
        GuardianStatus::Invited,
        "Guardian status should be 'Invited'"
    );
    assert_eq!(guardian.lead_guardian, false, "Should not be lead guardian");
    assert_eq!(
        guardian.invitation_id, invitation_id,
        "Guardian invitation_id should match event"
    );
}

#[tokio::test]
async fn test_invitation_created_handler_without_name() {
    // Create test store
    let store = create_test_store().await;

    // Create test SNS event for invitation_created without invited_name
    let box_id = "test_box_create_no_name";
    let invitation_id = "test_invitation_create_no_name";
    let event = create_test_sns_event_with_name(
        "invitation_created",
        invitation_id,
        box_id,
        "",   // No user_id for invitation_created events
        None, // No invited_name - should use fallback
    );

    // Create a test box record without any guardians initially
    let box_record = lockbox_shared::models::BoxRecord {
        id: box_id.to_string(),
        name: "Test Box".to_string(),
        description: "Test Description".to_string(),
        is_locked: false,
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        owner_id: "test_owner".to_string(),
        owner_name: Some("Test Owner".to_string()),
        documents: vec![],
        guardians: vec![], // Start with no guardians
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
    };

    // Add test box to store
    let _ = store.create_box(box_record).await.unwrap();

    // Call handler with the appropriate box store
    let result = store.handle_event(event).await;
    assert!(result.is_ok(), "Handler failed: {:?}", result.err());

    // Verify guardian was created with fallback name
    let box_result = store.get_box(box_id).await;
    assert!(box_result.is_ok());

    let box_record = box_result.unwrap();
    assert_eq!(box_record.guardians.len(), 1);

    let guardian = &box_record.guardians[0];

    // Verify fallback name format: "Pending (CODE)"
    assert!(
        guardian.name.starts_with("Pending ("),
        "Guardian name should use fallback format when invited_name is None"
    );
    assert_eq!(guardian.status, GuardianStatus::Invited);
    assert_eq!(guardian.invitation_id, invitation_id);
}

#[tokio::test]
async fn test_invitation_created_idempotency() {
    // Create test store
    let store = create_test_store().await;

    let box_id = "test_box_idempotent";
    let invitation_id = "test_invitation_idempotent";
    let invited_name = "Bob Jones";

    // Create a test box with a guardian already having this invitation_id
    let existing_guardian = lockbox_shared::models::Guardian {
        id: String::new(),
        name: invited_name.to_string(),
        lead_guardian: false,
        status: GuardianStatus::Invited,
        added_at: "2023-01-01T00:00:00Z".to_string(),
        invitation_id: invitation_id.to_string(),
    };

    let box_record = lockbox_shared::models::BoxRecord {
        id: box_id.to_string(),
        name: "Test Box".to_string(),
        description: "Test Description".to_string(),
        is_locked: false,
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        owner_id: "test_owner".to_string(),
        owner_name: Some("Test Owner".to_string()),
        documents: vec![],
        guardians: vec![existing_guardian],
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
    };

    // Add test box to store
    let _ = store.create_box(box_record).await.unwrap();

    // Send invitation_created event for the same invitation_id
    let event = create_test_sns_event_with_name(
        "invitation_created",
        invitation_id,
        box_id,
        "",
        Some(invited_name.to_string()),
    );

    // Call handler - should be idempotent
    let result = store.handle_event(event).await;
    assert!(
        result.is_ok(),
        "Handler should succeed for duplicate invitation_created event"
    );

    // Verify no duplicate guardian was created
    let box_result = store.get_box(box_id).await;
    assert!(box_result.is_ok());

    let box_record = box_result.unwrap();
    assert_eq!(
        box_record.guardians.len(),
        1,
        "Should still have exactly one guardian (no duplicate)"
    );

    let guardian = &box_record.guardians[0];
    assert_eq!(guardian.invitation_id, invitation_id);
    assert_eq!(guardian.name, invited_name);
}

#[tokio::test]
async fn test_invitation_created_lead_guardian() {
    // Create test store
    let store = create_test_store().await;

    // Create test SNS event for invitation_created with lead_guardian=true
    let box_id = "test_box_lead_guardian";
    let invitation_id = "test_invitation_lead_guardian";
    let invited_name = "Lead Guardian";
    let event = create_test_sns_event_full(
        "invitation_created",
        invitation_id,
        box_id,
        "", // No user_id for invitation_created events
        Some(invited_name.to_string()),
        true, // is_lead_guardian = true
    );

    // Create a test box record without any guardians initially
    let box_record = lockbox_shared::models::BoxRecord {
        id: box_id.to_string(),
        name: "Test Box".to_string(),
        description: "Test Description".to_string(),
        is_locked: false,
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        owner_id: "test_owner".to_string(),
        owner_name: Some("Test Owner".to_string()),
        documents: vec![],
        guardians: vec![], // Start with no guardians
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
    };

    // Add test box to store
    let _ = store.create_box(box_record).await.unwrap();

    // Call handler with the appropriate box store
    let result = store.handle_event(event).await;
    assert!(result.is_ok(), "Handler failed: {:?}", result.err());

    // Verify guardian was created correctly with lead_guardian=true
    let box_result = store.get_box(box_id).await;
    assert!(box_result.is_ok());

    let box_record = box_result.unwrap();
    assert_eq!(box_record.guardians.len(), 1);

    let guardian = &box_record.guardians[0];
    assert_eq!(guardian.name, invited_name);
    assert_eq!(
        guardian.lead_guardian, true,
        "Guardian should be marked as lead guardian"
    );
    assert_eq!(guardian.status, GuardianStatus::Invited);
    assert_eq!(guardian.invitation_id, invitation_id);
}

#[tokio::test]
async fn test_invitation_viewed_handler() {
    // Create test store
    let store = create_test_store().await;

    // Create test SNS event for invitation_viewed
    let box_id = "test_box_789";
    let invitation_id = "test_invitation_101";
    let user_id = "test_user_1"; // This should match the user_id in the test event
    let event = create_test_sns_event("invitation_viewed", invitation_id, box_id, user_id);

    // Create a test box record with a guardian with the test invitation_id
    let mut box_record = lockbox_shared::models::BoxRecord {
        id: box_id.to_string(),
        name: "Test Box".to_string(),
        description: "Test Description".to_string(),
        is_locked: false,
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        owner_id: "test_owner".to_string(),
        owner_name: Some("Test Owner".to_string()),
        documents: vec![],
        guardians: vec![],
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
    };

    // Add a guardian with the invitation_id but without a user_id yet
    // This simulates a guardian that was added to the box but invitation wasn't viewed yet
    let guardian = lockbox_shared::models::Guardian {
        id: "placeholder_id".to_string(),
        name: "Test Guardian".to_string(),
        lead_guardian: false,
        status: GuardianStatus::Invited,
        added_at: "2023-01-01T00:00:00Z".to_string(),
        invitation_id: invitation_id.to_string(), // Use the same invitation_id as in the event
    };

    box_record.guardians.push(guardian);

    // Add test box to store (works for both types)
    let _ = store.create_box(box_record).await.unwrap();

    // Call handler with the appropriate box store
    let result = store.handle_event(event).await;
    assert!(result.is_ok(), "Handler failed: {:?}", result.err());

    // Verify box was updated correctly after invitation viewed event
    let box_result = store.get_box(box_id).await;
    assert!(
        box_result.is_ok(),
        "Failed to retrieve box: {:?}",
        box_result.err()
    );

    // Get the box record and examine it
    let box_record = box_result.unwrap();

    // Find the guardian with the matching invitation_id
    let guardian = box_record
        .guardians
        .iter()
        .find(|g| g.invitation_id == invitation_id)
        .expect("Guardian with matching invitation_id should exist");

    // Verify that the user_id has been set correctly
    assert_eq!(
        guardian.id, user_id,
        "Guardian user_id should be updated to match the event's user_id"
    );

    // Verify that the status has been updated to "viewed"
    assert_eq!(
        guardian.status,
        GuardianStatus::Viewed,
        "Guardian status should be updated to 'viewed'"
    );
}

#[tokio::test]
async fn test_no_matching_guardian() {
    // Create test store
    let store = create_test_store().await;

    // Create test SNS event with non-existent invitation_id
    let box_id = "test_box_123";
    let invitation_id = "non_existent_invitation";
    let event = create_test_sns_event("invitation_viewed", invitation_id, box_id, "test_user_1");

    // Create a test box record without any guardian matching the invitation_id
    let box_record = lockbox_shared::models::BoxRecord {
        id: box_id.to_string(),
        name: "Test Box".to_string(),
        description: "Test Description".to_string(),
        is_locked: false,
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        owner_id: "test_owner".to_string(),
        owner_name: Some("Test Owner".to_string()),
        documents: vec![],
        guardians: vec![lockbox_shared::models::Guardian {
            id: "placeholder_id".to_string(),
            name: "Existing Guardian".to_string(),
            lead_guardian: false,
            status: GuardianStatus::Invited,
            added_at: "2023-01-01T00:00:00Z".to_string(),
            invitation_id: "different_invitation_id".to_string(),
        }],
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
    };

    // Add test box to store
    let _ = store.create_box(box_record).await.unwrap();

    // Call handler with the appropriate box store
    // This should handle the case gracefully (log a warning but not error)
    let result = store.handle_event(event).await;

    // The handler should not return an error even if no matching guardian is found
    assert!(
        result.is_ok(),
        "Handler should not fail when no matching guardian: {:?}",
        result.err()
    );

    // Box should remain unchanged
    let box_result = store.get_box(box_id).await;
    assert!(box_result.is_ok());
    let box_record = box_result.unwrap();

    // Verify guardian wasn't changed
    let guardian = &box_record.guardians[0];
    assert_eq!(guardian.invitation_id, "different_invitation_id");
    assert_eq!(guardian.id, "placeholder_id");
    assert_eq!(guardian.status, GuardianStatus::Invited);
}

#[tokio::test]
#[ignore]
async fn test_concurrent_updates() {
    // This test requires DynamoDB for proper concurrency testing
    // if !use_dynamodb() {
    //     log::info!("Skipping concurrent update test in mock mode");
    //     return;
    // }

    // Create DynamoDB test store
    let store = create_test_store().await;

    // Create a box with multiple guardians that have different invitation_ids
    let box_id = "test_box_concurrent";

    // Create three different invitation IDs
    let invitation_id1 = "test_invitation_concurrent_1";
    let invitation_id2 = "test_invitation_concurrent_2";
    let invitation_id3 = "test_invitation_concurrent_3";

    let mut box_record = lockbox_shared::models::BoxRecord {
        id: box_id.to_string(),
        name: "Concurrent Test Box".to_string(),
        description: "Test Description".to_string(),
        is_locked: false,
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        owner_id: "test_owner".to_string(),
        owner_name: Some("Test Owner".to_string()),
        documents: vec![],
        guardians: vec![],
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
    };

    // Add three guardians with different invitation_ids
    let guardian1 = lockbox_shared::models::Guardian {
        id: "placeholder_id_1".to_string(),
        name: "Test Guardian 1".to_string(),
        lead_guardian: false,
        status: GuardianStatus::Invited,
        added_at: "2023-01-01T00:00:00Z".to_string(),
        invitation_id: invitation_id1.to_string(),
    };

    let guardian2 = lockbox_shared::models::Guardian {
        id: "placeholder_id_2".to_string(),
        name: "Test Guardian 2".to_string(),
        lead_guardian: false,
        status: GuardianStatus::Invited,
        added_at: "2023-01-01T00:00:00Z".to_string(),
        invitation_id: invitation_id2.to_string(),
    };

    let guardian3 = lockbox_shared::models::Guardian {
        id: "placeholder_id_3".to_string(),
        name: "Test Guardian 3".to_string(),
        lead_guardian: false,
        status: GuardianStatus::Invited,
        added_at: "2023-01-01T00:00:00Z".to_string(),
        invitation_id: invitation_id3.to_string(),
    };

    box_record.guardians.push(guardian1);
    box_record.guardians.push(guardian2);
    box_record.guardians.push(guardian3);

    // Add test box to store
    let _ = store.create_box(box_record).await.unwrap();

    // Create events for each invitation ID
    let event1 = create_test_sns_event("invitation_viewed", invitation_id1, box_id, "test_user_1");
    let event2 = create_test_sns_event("invitation_viewed", invitation_id2, box_id, "test_user_2");
    let event3 = create_test_sns_event("invitation_viewed", invitation_id3, box_id, "test_user_3");

    // Process events concurrently
    let store_clone1 = store.clone();
    let store_clone2 = store.clone();
    let store_clone3 = store.clone();

    let handle1 = tokio::spawn(async move { store_clone1.handle_event(event1).await });

    let handle2 = tokio::spawn(async move { store_clone2.handle_event(event2).await });

    let handle3 = tokio::spawn(async move { store_clone3.handle_event(event3).await });

    // Wait for all handlers to complete
    let result1 = handle1.await.unwrap();
    let result2 = handle2.await.unwrap();
    let result3 = handle3.await.unwrap();

    // All handlers should succeed even with concurrent operations
    assert!(result1.is_ok(), "First handler failed: {:?}", result1.err());
    assert!(
        result2.is_ok(),
        "Second handler failed: {:?}",
        result2.err()
    );
    assert!(result3.is_ok(), "Third handler failed: {:?}", result3.err());

    // Verify final state - all guardians should be updated
    let box_result = store.get_box(box_id).await;
    assert!(box_result.is_ok());
    let box_record = box_result.unwrap();

    // Verify each specific guardian
    let guardian1 = box_record
        .guardians
        .iter()
        .find(|g| g.invitation_id == invitation_id1)
        .expect("Guardian with invitation_id1 should exist");

    let guardian2 = box_record
        .guardians
        .iter()
        .find(|g| g.invitation_id == invitation_id2)
        .expect("Guardian with invitation_id2 should exist");

    let guardian3 = box_record
        .guardians
        .iter()
        .find(|g| g.invitation_id == invitation_id3)
        .expect("Guardian with invitation_id3 should exist");

    assert_eq!(guardian1.status, GuardianStatus::Viewed);
    assert_eq!(guardian1.id, "test_user_1");

    assert_eq!(guardian2.status, GuardianStatus::Viewed);
    assert_eq!(guardian2.id, "test_user_2");

    assert_eq!(guardian3.status, GuardianStatus::Viewed);
    assert_eq!(guardian3.id, "test_user_3");
}

#[tokio::test]
async fn test_malformed_event() {
    // Create test store
    let store = create_test_store().await;

    // Create a valid box record
    let box_id = "test_box_malformed";
    let invitation_id = "test_invitation_malformed";

    // Create box with a single guardian
    let box_record = lockbox_shared::models::BoxRecord {
        id: box_id.to_string(),
        name: "Test Box".to_string(),
        description: "Test Description".to_string(),
        is_locked: false,
        created_at: "2023-01-01T00:00:00Z".to_string(),
        updated_at: "2023-01-01T00:00:00Z".to_string(),
        owner_id: "test_owner".to_string(),
        owner_name: Some("Test Owner".to_string()),
        documents: vec![],
        guardians: vec![lockbox_shared::models::Guardian {
            id: "placeholder_id".to_string(),
            name: "Test Guardian".to_string(),
            lead_guardian: false,
            status: GuardianStatus::Invited,
            added_at: "2023-01-01T00:00:00Z".to_string(),
            invitation_id: invitation_id.to_string(),
        }],
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
    };

    // Add test box to store
    let _ = store.create_box(box_record).await.unwrap();

    // Get original box state to compare later
    let original_box = store.get_box(box_id).await.unwrap();

    // Create a malformed SNS event with invalid JSON message
    let sns_message = SnsMessage {
        signature: "test-signature".to_string(),
        message_id: "test-message-id".to_string(),
        topic_arn: "arn:aws:sns:us-east-1:123456789012:invitation-events".to_string(),
        subject: Some("Invitation Event".to_string()),
        message: "{invalid_json: this-is-not-valid-json".to_string(), // Invalid JSON
        timestamp: Utc::now(),
        signature_version: "1".to_string(),
        signing_cert_url: "https://sns.us-east-1.amazonaws.com/cert.pem".to_string(),
        unsubscribe_url: "https://sns.us-east-1.amazonaws.com/unsubscribe".to_string(),
        message_attributes: HashMap::new(),
        sns_message_type: "Notification".to_string(),
    };

    // Create SNS record
    let sns_record = SnsRecord {
        event_version: "1.0".to_string(),
        event_subscription_arn:
            "arn:aws:sns:us-east-1:123456789012:invitation-events:subscription-id".to_string(),
        event_source: "aws:sns".to_string(),
        sns: sns_message,
    };

    // Create SNS event
    let sns_event = SnsEvent {
        records: vec![sns_record],
    };

    // Create Lambda event
    let event = LambdaEvent {
        payload: sns_event,
        context: lambda_runtime::Context::default(),
    };

    let result = store.handle_event(event).await;

    // With the updated handler behavior, malformed events are skipped and don't cause an error
    assert!(
        result.is_ok(),
        "Handler should continue processing even with malformed event"
    );

    // Verify the box data wasn't changed
    let box_result = store.get_box(box_id).await;
    assert!(
        box_result.is_ok(),
        "Should still be able to retrieve the box"
    );
    let box_record = box_result.unwrap();

    // Box should be unchanged - verify guardian status and ID
    assert_eq!(box_record.guardians.len(), original_box.guardians.len());
    let guardian = &box_record.guardians[0];
    assert_eq!(guardian.invitation_id, invitation_id);
    assert_eq!(guardian.id, "placeholder_id");
    assert_eq!(guardian.status, GuardianStatus::Invited);

    // Verify no other fields were changed
    assert_eq!(box_record.id, original_box.id);
    assert_eq!(box_record.name, original_box.name);
    assert_eq!(box_record.description, original_box.description);
    assert_eq!(box_record.is_locked, original_box.is_locked);
}

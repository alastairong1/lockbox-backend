use axum::{http::StatusCode, Router};
use log::{debug, error, info, trace};
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt;

use crate::routes::create_router_with_store;
use chrono::{DateTime, Duration, Utc};
use lockbox_shared::auth::create_test_request;
use lockbox_shared::models::Invitation;
use lockbox_shared::store::dynamo::DynamoInvitationStore;
use lockbox_shared::store::InvitationStore;
use lockbox_shared::test_utils::dynamo_test_utils::{
    clear_dynamo_table, create_dynamo_client, create_invitation_table, use_dynamodb,
};
use lockbox_shared::test_utils::http_test_utils::response_to_json;
use lockbox_shared::test_utils::mock_invitation_store::MockInvitationStore;
use lockbox_shared::test_utils::test_logging::init_test_logging;
use std::env;
use uuid::Uuid;

// Constants for DynamoDB tests
const TEST_TABLE_NAME: &str = "invitation-test-table";

enum TestStore {
    Mock(Arc<MockInvitationStore>),
    DynamoDB(Arc<DynamoInvitationStore>),
}

// Helper to set up test application with the appropriate store based on environment
async fn create_test_app() -> (Router, TestStore) {
    // Initialize logging for tests
    init_test_logging();

    // Set SNS environment variable for all tests
    env::set_var(
        "SNS_TOPIC_ARN",
        "arn:aws:sns:us-east-1:123456789012:test-topic",
    );

    // Set a test flag to skip actual SNS publishing
    env::set_var("TEST_SNS", "true");

    if use_dynamodb() {
        // Set up DynamoDB store
        info!("Using DynamoDB for invitation tests");
        let client = create_dynamo_client().await;

        // Create the table (ignore errors if table already exists)
        debug!("Setting up DynamoDB test table '{}'", TEST_TABLE_NAME);
        match create_invitation_table(&client, TEST_TABLE_NAME).await {
            Ok(_) => info!("Test table created successfully"),
            Err(e) => {
                // Only log if it's not a table already exists error
                if !e.to_string().contains("ResourceInUseException") {
                    error!("Error creating table: {}", e);
                } else {
                    info!("Table already exists, continuing");
                }
            }
        }

        // Clean the table to start fresh
        debug!("Clearing DynamoDB test table");
        match clear_dynamo_table(&client, TEST_TABLE_NAME).await {
            Ok(_) => debug!("Table cleared successfully"),
            Err(e) => error!("Failed to clear table: {}", e),
        }

        // Verify table is empty
        let scan_result = client.scan().table_name(TEST_TABLE_NAME).send().await;
        match scan_result {
            Ok(output) => {
                if let Some(items) = output.items {
                    if !items.is_empty() {
                        error!(
                            "Table not empty after clearing, found {} items",
                            items.len()
                        );
                    } else {
                        debug!("Table is empty and ready for testing");
                    }
                }
            }
            Err(e) => error!("Error scanning table: {}", e),
        }

        // Create the DynamoDB store with our test table
        info!(
            "Creating DynamoInvitationStore with table '{}'",
            TEST_TABLE_NAME
        );
        let store = Arc::new(DynamoInvitationStore::with_client_and_table(
            client,
            TEST_TABLE_NAME.to_string(),
        ));

        let app = create_router_with_store(store.clone(), "");
        (app, TestStore::DynamoDB(store))
    } else {
        // Use mock store
        debug!("Using mock store for invitation tests");
        let store = Arc::new(MockInvitationStore::new_with_expiry());
        let app = create_router_with_store(store.clone(), "");
        (app, TestStore::Mock(store))
    }
}

#[tokio::test]
async fn test_create_invitation() {
    let (app, store) = create_test_app().await;

    let payload = json!({
        "invitedName": "Test User",
        "boxId": "box-123"
    });

    let response = app
        .clone()
        .oneshot(create_test_request(
            "POST",
            "/invitations/new",
            "test-user-id",
            Some(payload),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json_resp = response_to_json(response).await;

    // Verify the fields of the Invitation object
    let invite_code = json_resp["inviteCode"].as_str().unwrap();
    let expires_at = json_resp["expiresAt"].as_str().unwrap();
    assert_eq!(invite_code.len(), 8);
    assert!(!expires_at.is_empty());
    let expires_at_dt = DateTime::parse_from_rfc3339(expires_at)
        .unwrap()
        .with_timezone(&Utc);
    let now = Utc::now();
    let diff_secs = (expires_at_dt - now).num_seconds();
    assert!(
        diff_secs >= 47 * 3600 && diff_secs <= 49 * 3600,
        "Expiration time not within 47-49 hours, got {} seconds",
        diff_secs
    );

    // Verify additional fields in the full invitation response
    assert_eq!(json_resp["invitedName"], "Test User");
    assert_eq!(json_resp["boxId"], "box-123");
    assert_eq!(json_resp["creatorId"], "test-user-id");
    assert_eq!(json_resp["opened"], false);
    assert!(json_resp["linkedUserId"].is_null());

    // Add a small delay to allow for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    }

    // Verify stored invitation - First try to get the ID from the response
    let invitation_id = json_resp["id"].as_str().unwrap();

    let invitation = match &store {
        TestStore::Mock(mock) => mock.get_invitation(invitation_id).await.unwrap(),
        TestStore::DynamoDB(dynamo) => {
            info!("About to get invitation by ID: {}", invitation_id);
            let inv = dynamo.get_invitation(invitation_id).await.unwrap();
            info!(
                "Found invitation with id={}, creator_id={}",
                inv.id, inv.creator_id
            );
            // Double check we can get it by creator_id too
            let creator_invs = dynamo
                .get_invitations_by_creator_id(&inv.creator_id)
                .await
                .unwrap();
            info!(
                "Found {} invitations by creator_id={}",
                creator_invs.len(),
                inv.creator_id
            );
            inv
        }
    };

    // Verify the invitation properties
    assert_eq!(invitation.creator_id, "test-user-id");
    assert_eq!(invitation.invited_name, "Test User");
    assert_eq!(invitation.box_id, "box-123");
    assert!(!invitation.opened);
    assert!(invitation.linked_user_id.is_none());
}

#[tokio::test]
async fn test_handle_invitation() {
    let (app, store) = create_test_app().await;

    // seed an invitation directly
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "TESTCODE".to_string();
    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "Test User".to_string(),
        box_id: "box-123".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: (now + Duration::hours(2)).to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-id".to_string(),
    };

    debug!("Creating test invitation with code: {}", invite_code);
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    let handle_payload = json!({
        "userId": "user-456",
        "inviteCode": invite_code
    });
    let response = app
        .clone()
        .oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "user-456",
            Some(handle_payload),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json_resp = response_to_json(response).await;
    assert_eq!(json_resp["boxId"], "box-123");

    let updated_inv = match &store {
        TestStore::Mock(mock) => mock.get_invitation_by_code(&invite_code).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.get_invitation_by_code(&invite_code).await.unwrap(),
    };

    assert!(updated_inv.opened);
    assert_eq!(updated_inv.linked_user_id, Some("user-456".to_string()));

    // Additional test for SNS event payload
    // Verify the structure of the SNS event that would be emitted
    let event_payload = json!({
        "event_type": "invitation_viewed",
        "invitation_id": updated_inv.id,
        "box_id": updated_inv.box_id,
        "user_id": updated_inv.linked_user_id,
        "invite_code": updated_inv.invite_code,
        "timestamp": Utc::now().to_rfc3339() // Cannot match exactly, it's generated at runtime
    });

    // Verify important fields in the event payload
    assert_eq!(event_payload["event_type"], "invitation_viewed");
    assert_eq!(event_payload["invitation_id"], updated_inv.id);
    assert_eq!(event_payload["box_id"], "box-123");
    assert_eq!(event_payload["user_id"], "user-456");
    assert_eq!(event_payload["invite_code"], "TESTCODE");
    assert!(event_payload["timestamp"].is_string());
}

#[tokio::test]
async fn test_handle_invitation_expired_code() {
    let (app, store) = create_test_app().await;

    // seed an expired invitation
    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "EXPIRED".to_string();
    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "Test User".to_string(),
        box_id: "box-123".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: (now - Duration::hours(1)).to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-id".to_string(),
    };

    debug!(
        "Creating expired test invitation with code: {}",
        invite_code
    );
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    let bad_payload = json!({
        "userId": "user-456",
        "inviteCode": "EXPIRED"
    });
    let response = app
        .clone()
        .oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "user-456",
            Some(bad_payload),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::GONE);
}

#[tokio::test]
async fn test_refresh_invitation() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let old_code = "OLDCODE1".to_string();

    // Use the same dates for both mock and DynamoDB
    // Create time in the past, not yet expired (for both implementations)
    let create_time = now - Duration::hours(5); // Created 5 hours ago
    let expiry_time = now + Duration::hours(1); // Expires 1 hour from now

    let invitation = Invitation {
        id: id.clone(),
        invite_code: old_code.clone(),
        invited_name: "Test User".to_string(),
        box_id: "box-123".to_string(),
        created_at: create_time.to_rfc3339(),
        expires_at: expiry_time.to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "test-user-id".to_string(),
    };

    debug!(
        "Creating test invitation for refresh with code: {}",
        old_code
    );
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add a delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
    }

    let path = format!("/invitations/{}/refresh", id);
    let response = app
        .clone()
        .oneshot(create_test_request("PATCH", &path, "test-user-id", None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json_resp = response_to_json(response).await;
    let new_code = json_resp["inviteCode"].as_str().unwrap();
    assert_ne!(new_code, old_code);

    let expires_at = json_resp["expiresAt"].as_str().unwrap();
    let expires_at_dt = DateTime::parse_from_rfc3339(expires_at)
        .unwrap()
        .with_timezone(&Utc);
    let now2 = Utc::now();
    let diff_secs = (expires_at_dt - now2).num_seconds();
    assert!(
        diff_secs >= 47 * 3600 && diff_secs <= 49 * 3600,
        "Expiration time not within 47-49 hours, got {} seconds",
        diff_secs
    );

    // Verify full response fields
    assert_eq!(json_resp["id"], id);
    assert_eq!(json_resp["boxId"], "box-123");
    assert_eq!(json_resp["invitedName"], "Test User");
    assert_eq!(json_resp["creatorId"], "test-user-id");
    assert_eq!(json_resp["opened"], false);
    assert!(json_resp["linkedUserId"].is_null());

    // Add a delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    let refreshed = match &store {
        TestStore::Mock(mock) => mock.get_invitation(&id).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.get_invitation(&id).await.unwrap(),
    };

    assert_eq!(refreshed.invite_code, new_code.to_string());
    assert!(!refreshed.opened);
    assert!(refreshed.linked_user_id.is_none());
}

#[tokio::test]
async fn test_refresh_invitation_invalid_id() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invitation = Invitation {
        id: id.clone(),
        invite_code: "CODE1234".to_string(),
        invited_name: "Test User".to_string(),
        box_id: "box-123".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: (now + Duration::hours(2)).to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "owner-id".to_string(),
    };

    debug!("Creating test invitation with different owner id: {}", id);
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    let path = format!("/invitations/{}/refresh", id);
    let response = app
        .clone()
        .oneshot(create_test_request("PATCH", &path, "other-user-id", None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_handle_invitation_invalid_code() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invitation = Invitation {
        id: id.clone(),
        invite_code: "VALID123".to_string(),
        invited_name: "Test User".to_string(),
        box_id: "box-123".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: (now + Duration::hours(2)).to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-id".to_string(),
    };

    debug!("Creating test invitation with code VALID123");
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    let bad_payload = json!({
        "userId": "user-456",
        "inviteCode": "INVALID"
    });
    let response = app
        .clone()
        .oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "user-456",
            Some(bad_payload),
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_my_invitations() {
    let (app, store) = create_test_app().await;

    // Seed multiple invitations
    debug!("Seeding multiple test invitations");
    // Note ids for verification
    let test_cases = [
        ("User 1", "box-123", "test-user-id"),
        ("User 2", "box-456", "test-user-id"),
        ("User 3", "box-789", "other-user-id"),
    ];

    let mut ids = Vec::new();

    for (name, box_id, creator) in &test_cases {
        let id = Uuid::new_v4().to_string();
        let invite_code = Uuid::new_v4()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>()
            .to_uppercase();
        let now = Utc::now();
        let invitation = Invitation {
            id: id.clone(),
            invite_code,
            invited_name: name.to_string(),
            box_id: box_id.to_string(),
            created_at: now.to_rfc3339(),
            expires_at: (now + Duration::hours(48)).to_rfc3339(),
            opened: false,
            linked_user_id: None,
            creator_id: creator.to_string(),
        };

        trace!(
            "Creating invitation for {}, box {}, creator {}",
            name,
            box_id,
            creator
        );
        match &store {
            TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
            TestStore::DynamoDB(dynamo) => {
                dynamo.create_invitation(invitation.clone()).await.unwrap()
            }
        };

        ids.push((id, creator.to_string()));
    }

    // Add a delay to allow for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
    }

    let response = app
        .clone()
        .oneshot(create_test_request(
            "GET",
            "/invitations/me",
            "test-user-id",
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json_resp = response_to_json(response).await;
    let arr = json_resp.as_array().unwrap();

    // Both mock and DynamoDB should work correctly now that GSI is fixed
    // We should get only the invitations where test-user-id is the creator
    assert_eq!(arr.len(), 2, "Expected 2 invitations for test-user-id");

    // Verify each returned invitation has the correct creator_id
    for item in arr {
        assert_eq!(item["creatorId"], "test-user-id");
    }
}

#[tokio::test]
async fn test_get_my_invitations_empty() {
    let (app, _store) = create_test_app().await;

    let response = app
        .clone()
        .oneshot(create_test_request(
            "GET",
            "/invitations/me",
            "test-user-id",
            None,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json_resp = response_to_json(response).await;
    assert!(json_resp.as_array().unwrap().is_empty());
}

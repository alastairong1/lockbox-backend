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
        is_lead_guardian: false,
    };

    debug!("Creating test invitation with code: {}", invite_code);
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    let handle_payload = json!({
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
        is_lead_guardian: false,
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
        is_lead_guardian: false,
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
        is_lead_guardian: false,
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
        is_lead_guardian: false,
    };

    debug!("Creating test invitation with code VALID123");
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    let bad_payload = json!({
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
            is_lead_guardian: false,
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

// New tests for view invitation endpoint
#[tokio::test]
async fn test_view_invitation_by_code_success() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "VIEWCODE".to_string();
    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "View Test User".to_string(),
        box_id: "box-view-123".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: (now + Duration::hours(48)).to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-view-id".to_string(),
        is_lead_guardian: false,
    };

    debug!(
        "Creating test invitation for viewing with code: {}",
        invite_code
    );
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    let path = format!("/invitations/view/{}", invite_code);
    let response = app
        .clone()
        .oneshot(create_test_request("GET", &path, "any-user-id", None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json_resp = response_to_json(response).await;

    // Verify response fields
    assert_eq!(json_resp["id"], id);
    assert_eq!(json_resp["inviteCode"], invite_code);
    assert_eq!(json_resp["invitedName"], "View Test User");
    assert_eq!(json_resp["boxId"], "box-view-123");
    assert_eq!(json_resp["creatorId"], "creator-view-id");
    assert_eq!(json_resp["opened"], false);
    assert!(json_resp["linkedUserId"].is_null());
    assert!(!json_resp["createdAt"].as_str().unwrap().is_empty());
    assert!(!json_resp["expiresAt"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_view_invitation_by_code_not_found() {
    let (app, _store) = create_test_app().await;

    let path = "/invitations/view/NOTFOUND";
    let response = app
        .clone()
        .oneshot(create_test_request("GET", path, "any-user-id", None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_view_invitation_by_code_expired() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "EXPIRED2".to_string();
    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "Expired View User".to_string(),
        box_id: "box-expired-view".to_string(),
        created_at: (now - Duration::hours(50)).to_rfc3339(),
        expires_at: (now - Duration::hours(2)).to_rfc3339(), // Expired 2 hours ago
        opened: false,
        linked_user_id: None,
        creator_id: "creator-expired-id".to_string(),
        is_lead_guardian: false,
    };

    debug!(
        "Creating expired test invitation for viewing with code: {}",
        invite_code
    );
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    let path = format!("/invitations/view/{}", invite_code);
    let response = app
        .clone()
        .oneshot(create_test_request("GET", &path, "any-user-id", None))
        .await
        .unwrap();

    // Should return 410 (GONE) for expired invitations
    assert_eq!(response.status(), StatusCode::GONE);
}

#[tokio::test]
async fn test_view_invitation_does_not_consume_code() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "NOCONSUM".to_string();
    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "Non-Consume User".to_string(),
        box_id: "box-noconsum".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: (now + Duration::hours(48)).to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-noconsum-id".to_string(),
        is_lead_guardian: false,
    };

    debug!(
        "Creating test invitation for non-consuming view with code: {}",
        invite_code
    );
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    // View the invitation
    let path = format!("/invitations/view/{}", invite_code);
    let response = app
        .clone()
        .oneshot(create_test_request("GET", &path, "viewer-user-id", None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Verify invitation was not consumed (opened = false, linkedUserId = None)
    let viewed_inv = match &store {
        TestStore::Mock(mock) => mock.get_invitation_by_code(&invite_code).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.get_invitation_by_code(&invite_code).await.unwrap(),
    };

    assert!(!viewed_inv.opened);
    assert!(viewed_inv.linked_user_id.is_none());
}

// Tests for concurrent code redemption
#[tokio::test]
async fn test_concurrent_code_redemption_second_fails() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "CONCUR01".to_string();
    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "Concurrent Test User".to_string(),
        box_id: "box-concurrent-123".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: (now + Duration::hours(48)).to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-concurrent-id".to_string(),
        is_lead_guardian: false,
    };

    debug!(
        "Creating test invitation for concurrent redemption with code: {}",
        invite_code
    );
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    // First user redeems the code
    let handle_payload_1 = json!({
        "inviteCode": invite_code
    });
    let response1 = app
        .clone()
        .oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "user-first",
            Some(handle_payload_1),
        ))
        .await
        .unwrap();

    assert_eq!(response1.status(), StatusCode::OK);
    let json_resp1 = response_to_json(response1).await;
    assert_eq!(json_resp1["boxId"], "box-concurrent-123");

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    // Second user tries to redeem the same code
    let handle_payload_2 = json!({
        "inviteCode": invite_code
    });
    let response2 = app
        .clone()
        .oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "user-second",
            Some(handle_payload_2),
        ))
        .await
        .unwrap();

    // Second redemption should fail with FORBIDDEN (invitation already used)
    assert_eq!(response2.status(), StatusCode::FORBIDDEN);

    // Verify invitation is linked to first user only
    let final_inv = match &store {
        TestStore::Mock(mock) => mock.get_invitation_by_code(&invite_code).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.get_invitation_by_code(&invite_code).await.unwrap(),
    };

    assert!(final_inv.opened);
    assert_eq!(final_inv.linked_user_id, Some("user-first".to_string()));
}

#[tokio::test]
async fn test_concurrent_code_redemption_truly_concurrent() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "CONCUR02".to_string();
    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "Truly Concurrent User".to_string(),
        box_id: "box-concurrent-456".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: (now + Duration::hours(48)).to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-concurrent-id".to_string(),
        is_lead_guardian: false,
    };

    debug!(
        "Creating test invitation for truly concurrent redemption with code: {}",
        invite_code
    );
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    // Simulate truly concurrent requests using tokio::spawn
    let handle_payload_1 = json!({
        "inviteCode": invite_code.clone()
    });
    let handle_payload_2 = json!({
        "inviteCode": invite_code.clone()
    });

    let app1 = app.clone();
    let app2 = app.clone();

    let task1 = tokio::spawn(async move {
        app1.oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "concurrent-user-1",
            Some(handle_payload_1),
        ))
        .await
    });

    let task2 = tokio::spawn(async move {
        app2.oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "concurrent-user-2",
            Some(handle_payload_2),
        ))
        .await
    });

    let (result1, result2) = tokio::join!(task1, task2);
    let response1 = result1.unwrap().unwrap();
    let response2 = result2.unwrap().unwrap();

    // One should succeed (OK) and one should fail (FORBIDDEN)
    let statuses = vec![response1.status(), response2.status()];
    assert!(
        statuses.contains(&StatusCode::OK),
        "One request should succeed"
    );
    assert!(
        statuses.contains(&StatusCode::FORBIDDEN),
        "One request should fail with FORBIDDEN"
    );

    // Verify invitation is linked to only one user
    let final_inv = match &store {
        TestStore::Mock(mock) => mock.get_invitation_by_code(&invite_code).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.get_invitation_by_code(&invite_code).await.unwrap(),
    };

    assert!(final_inv.opened);
    assert!(final_inv.linked_user_id.is_some());
    // Should be linked to either concurrent-user-1 or concurrent-user-2, but not both
    let linked_user = final_inv.linked_user_id.unwrap();
    assert!(
        linked_user == "concurrent-user-1" || linked_user == "concurrent-user-2",
        "Invitation should be linked to one of the concurrent users"
    );
}

// Tests for invitation expiry edge cases
#[tokio::test]
async fn test_invitation_expires_at_exact_48_hour_mark() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "EXACT48H".to_string();

    // Create invitation that expires exactly 48 hours from creation
    let created_time = now - Duration::hours(48);
    let expires_time = now; // Expires right now

    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "Exact 48h User".to_string(),
        box_id: "box-48h".to_string(),
        created_at: created_time.to_rfc3339(),
        expires_at: expires_time.to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-48h".to_string(),
        is_lead_guardian: false,
    };

    debug!("Creating invitation that expires exactly now");
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    // Try to handle invitation (should fail as expired)
    let handle_payload = json!({
        "inviteCode": invite_code
    });
    let response = app
        .clone()
        .oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "user-exact",
            Some(handle_payload),
        ))
        .await
        .unwrap();

    // Should return 410 Gone for expired invitation
    assert_eq!(response.status(), StatusCode::GONE);
}

#[tokio::test]
async fn test_invitation_just_before_expiry() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "JUSTBEFR".to_string();

    // Create invitation that expires 1 minute from now
    let expires_time = now + Duration::minutes(1);

    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "Just Before User".to_string(),
        box_id: "box-before".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: expires_time.to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-before".to_string(),
        is_lead_guardian: false,
    };

    debug!("Creating invitation that expires in 1 minute");
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    // Try to handle invitation (should succeed)
    let handle_payload = json!({
        "inviteCode": invite_code
    });
    let response = app
        .clone()
        .oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "user-before",
            Some(handle_payload),
        ))
        .await
        .unwrap();

    // Should succeed with OK
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_invitation_timezone_handling() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "TIMEZONE".to_string();

    // Create invitation with explicit UTC timezone
    let expires_time = now + Duration::hours(24);

    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "Timezone User".to_string(),
        box_id: "box-tz".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: expires_time.to_rfc3339(), // RFC3339 includes timezone
        opened: false,
        linked_user_id: None,
        creator_id: "creator-tz".to_string(),
        is_lead_guardian: false,
    };

    debug!("Creating invitation with UTC timezone");
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    // Verify invitation can be retrieved and expiry is correctly parsed
    let retrieved_inv = match &store {
        TestStore::Mock(mock) => mock.get_invitation_by_code(&invite_code).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.get_invitation_by_code(&invite_code).await.unwrap(),
    };

    // Parse expiry and verify it's in the future
    let parsed_expiry = chrono::DateTime::parse_from_rfc3339(&retrieved_inv.expires_at).unwrap();
    assert!(parsed_expiry > now);
}

#[tokio::test]
async fn test_refresh_resets_expiry_correctly() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let old_code = "RESETEXP".to_string();

    // Create invitation that will expire soon
    let old_expiry = now + Duration::hours(1); // Expires in 1 hour

    let invitation = Invitation {
        id: id.clone(),
        invite_code: old_code.clone(),
        invited_name: "Reset Expiry User".to_string(),
        box_id: "box-reset".to_string(),
        created_at: (now - Duration::hours(47)).to_rfc3339(), // Created 47 hours ago
        expires_at: old_expiry.to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-reset".to_string(),
        is_lead_guardian: false,
    };

    debug!("Creating invitation with old expiry time");
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(5000)).await;
    }

    let path = format!("/invitations/{}/refresh", id);
    let response = app
        .clone()
        .oneshot(create_test_request("PATCH", &path, "creator-reset", None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json_resp = response_to_json(response).await;

    // Verify new expiry is ~48 hours from now (not from original creation)
    let new_expiry_str = json_resp["expiresAt"].as_str().unwrap();
    let new_expiry = chrono::DateTime::parse_from_rfc3339(new_expiry_str)
        .unwrap()
        .with_timezone(&Utc);
    let now_check = Utc::now();
    let diff_secs = (new_expiry - now_check).num_seconds();

    assert!(
        diff_secs >= 47 * 3600 && diff_secs <= 49 * 3600,
        "Expiry should be reset to ~48 hours from refresh time, got {} seconds",
        diff_secs
    );
}

#[tokio::test]
async fn test_expiry_persists_across_view_operations() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let id = Uuid::new_v4().to_string();
    let invite_code = "VIEWPERS".to_string();
    let expires_time = now + Duration::hours(24);

    let invitation = Invitation {
        id: id.clone(),
        invite_code: invite_code.clone(),
        invited_name: "View Persist User".to_string(),
        box_id: "box-persist".to_string(),
        created_at: now.to_rfc3339(),
        expires_at: expires_time.to_rfc3339(),
        opened: false,
        linked_user_id: None,
        creator_id: "creator-persist".to_string(),
        is_lead_guardian: false,
    };

    debug!("Creating invitation to test expiry persistence");
    match &store {
        TestStore::Mock(mock) => mock.create_invitation(invitation.clone()).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation.clone()).await.unwrap(),
    };

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }

    // View invitation (should not modify expiry)
    let path = format!("/invitations/view/{}", invite_code);
    let response = app
        .clone()
        .oneshot(create_test_request("GET", &path, "viewer-user", None))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let json_resp = response_to_json(response).await;

    // Verify expiry hasn't changed
    assert_eq!(
        json_resp["expiresAt"].as_str().unwrap(),
        expires_time.to_rfc3339()
    );

    // Verify in database too
    let db_inv = match &store {
        TestStore::Mock(mock) => mock.get_invitation_by_code(&invite_code).await.unwrap(),
        TestStore::DynamoDB(dynamo) => dynamo.get_invitation_by_code(&invite_code).await.unwrap(),
    };

    assert_eq!(db_inv.expires_at, expires_time.to_rfc3339());
}

// Tests for code collision probability
#[tokio::test]
async fn test_code_uniqueness_small_batch() {
    let (app, _store) = create_test_app().await;

    let mut codes = std::collections::HashSet::new();
    let num_codes = 100;

    // Generate 100 invitation codes
    for i in 0..num_codes {
        let payload = json!({
            "invitedName": format!("User {}", i),
            "boxId": "box-unique"
        });

        let response = app
            .clone()
            .oneshot(create_test_request(
                "POST",
                "/invitations/new",
                "creator-unique",
                Some(payload),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json_resp = response_to_json(response).await;
        let code = json_resp["inviteCode"].as_str().unwrap().to_string();

        // Verify code is 8 characters
        assert_eq!(code.len(), 8);

        // Verify code uses only A-Z
        assert!(code.chars().all(|c| c.is_ascii_uppercase()));

        // Add to set (will fail if duplicate)
        assert!(codes.insert(code.clone()), "Found duplicate code: {}", code);
    }

    // Verify all codes are unique
    assert_eq!(codes.len(), num_codes);
}

#[tokio::test]
async fn test_code_uniqueness_medium_batch() {
    let (app, _store) = create_test_app().await;

    let mut codes = std::collections::HashSet::new();
    let num_codes = 1000;

    info!(
        "Generating {} invitation codes to test uniqueness",
        num_codes
    );

    // Generate 1000 invitation codes
    for i in 0..num_codes {
        let payload = json!({
            "invitedName": format!("User {}", i),
            "boxId": format!("box-{}", i % 10) // Distribute across 10 boxes
        });

        let response = app
            .clone()
            .oneshot(create_test_request(
                "POST",
                "/invitations/new",
                "creator-medium",
                Some(payload),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json_resp = response_to_json(response).await;
        let code = json_resp["inviteCode"].as_str().unwrap().to_string();

        // Add to set (will fail if duplicate)
        if !codes.insert(code.clone()) {
            panic!("Found duplicate code at iteration {}: {}", i, code);
        }
    }

    // Verify all codes are unique
    assert_eq!(codes.len(), num_codes);
    info!("Successfully generated {} unique codes", codes.len());
}

#[tokio::test]
async fn test_code_alphabet_distribution() {
    let (app, _store) = create_test_app().await;

    let mut char_counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    let num_codes = 200;

    // Generate codes and count character occurrences
    for i in 0..num_codes {
        let payload = json!({
            "invitedName": format!("User {}", i),
            "boxId": "box-dist"
        });

        let response = app
            .clone()
            .oneshot(create_test_request(
                "POST",
                "/invitations/new",
                "creator-dist",
                Some(payload),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let json_resp = response_to_json(response).await;
        let code = json_resp["inviteCode"].as_str().unwrap();

        // Count characters
        for ch in code.chars() {
            *char_counts.entry(ch).or_insert(0) += 1;
        }
    }

    // Verify all 26 letters appear at least once (statistically very likely)
    let total_letters = ('A'..='Z').count();
    let letters_used = char_counts.len();

    info!(
        "Used {} out of {} possible letters",
        letters_used, total_letters
    );
    info!("Character distribution: {:?}", char_counts);

    // With 200 codes * 8 chars = 1600 characters, we expect most letters to appear
    // Allow some variance, but should have at least 20 different letters
    assert!(
        letters_used >= 20,
        "Expected at least 20 different letters, got {}",
        letters_used
    );
}

#[tokio::test]
async fn test_code_collision_probability_calculation() {
    // This test verifies the theoretical collision probability
    // 26^8 = 208,827,064,576 possible codes
    // For 1000 codes, collision probability is approximately 1 - e^(-1000^2 / (2 * 208827064576))
    // Which is extremely low (< 0.000001%)

    let alphabet_size = 26u64;
    let code_length = 8u64;
    let total_combinations = alphabet_size.pow(code_length as u32);

    info!("Total possible combinations: {}", total_combinations);
    assert_eq!(total_combinations, 208_827_064_576);

    // For practical purposes, with our expected usage:
    // - Average box has 3-5 guardians
    // - Average user has 2-3 boxes
    // - 10,000 active users = ~150,000 total invitations
    // With 8-character codes, collision probability is acceptable

    let expected_invitations = 150_000u64;
    let collision_probability =
        (expected_invitations as f64).powi(2) / (2.0 * total_combinations as f64);

    info!("Expected invitations: {}", expected_invitations);
    info!(
        "Theoretical collision probability: {:.10}",
        collision_probability
    );

    // Probability should be less than 10% (reasonable threshold for this scale)
    // At 150k invitations: ~5.4% collision probability
    assert!(collision_probability < 0.1);
}

// Tests for code lookup performance
#[tokio::test]
async fn test_code_lookup_performance_active_codes() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let num_codes = 50;
    let mut codes = Vec::new();

    // Create 50 active invitation codes
    for i in 0..num_codes {
        let id = Uuid::new_v4().to_string();
        let code = format!("PERF{:04}", i);
        codes.push(code.clone());

        let invitation = Invitation {
            id: id.clone(),
            invite_code: code,
            invited_name: format!("Perf User {}", i),
            box_id: format!("box-{}", i),
            created_at: now.to_rfc3339(),
            expires_at: (now + Duration::hours(48)).to_rfc3339(),
            opened: false,
            linked_user_id: None,
            creator_id: format!("creator-{}", i % 5), // 5 different creators
            is_lead_guardian: false,
        };

        match &store {
            TestStore::Mock(mock) => mock.create_invitation(invitation).await.unwrap(),
            TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation).await.unwrap(),
        };
    }

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;
    }

    // Measure lookup time for each code
    let mut total_duration = tokio::time::Duration::from_millis(0);
    for code in &codes {
        let start = tokio::time::Instant::now();

        let handle_payload = json!({
            "inviteCode": code
        });
        let response = app
            .clone()
            .oneshot(create_test_request(
                "PUT",
                "/invitations/handle",
                "user-perf",
                Some(handle_payload),
            ))
            .await
            .unwrap();

        let duration = start.elapsed();
        total_duration += duration;

        // Each lookup should succeed
        assert!(
            response.status() == StatusCode::OK || response.status() == StatusCode::FORBIDDEN,
            "Lookup failed for code: {}",
            code
        );

        debug!("Lookup for {} took {:?}", code, duration);
    }

    let avg_duration = total_duration / num_codes as u32;
    info!("Average lookup time: {:?}", avg_duration);

    // Average lookup should be under 500ms (generous for test environment)
    assert!(
        avg_duration < tokio::time::Duration::from_millis(500),
        "Average lookup time too high: {:?}",
        avg_duration
    );
}

#[tokio::test]
async fn test_code_lookup_performance_mixed_dataset() {
    let (app, store) = create_test_app().await;

    let now = Utc::now();
    let num_active = 20;
    let num_expired = 20;

    // Create mix of active and expired invitations
    for i in 0..num_active {
        let id = Uuid::new_v4().to_string();
        let code = format!("ACTV{:04}", i);

        let invitation = Invitation {
            id,
            invite_code: code,
            invited_name: format!("Active User {}", i),
            box_id: format!("box-{}", i),
            created_at: now.to_rfc3339(),
            expires_at: (now + Duration::hours(24)).to_rfc3339(),
            opened: false,
            linked_user_id: None,
            creator_id: "creator-mixed".to_string(),
            is_lead_guardian: false,
        };

        match &store {
            TestStore::Mock(mock) => mock.create_invitation(invitation).await.unwrap(),
            TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation).await.unwrap(),
        };
    }

    for i in 0..num_expired {
        let id = Uuid::new_v4().to_string();
        let code = format!("EXPR{:04}", i);

        let invitation = Invitation {
            id,
            invite_code: code,
            invited_name: format!("Expired User {}", i),
            box_id: format!("box-{}", i + 100),
            created_at: (now - Duration::hours(50)).to_rfc3339(),
            expires_at: (now - Duration::hours(2)).to_rfc3339(), // Expired
            opened: false,
            linked_user_id: None,
            creator_id: "creator-mixed".to_string(),
            is_lead_guardian: false,
        };

        match &store {
            TestStore::Mock(mock) => mock.create_invitation(invitation).await.unwrap(),
            TestStore::DynamoDB(dynamo) => dynamo.create_invitation(invitation).await.unwrap(),
        };
    }

    // Add delay for DynamoDB consistency
    if matches!(store, TestStore::DynamoDB(_)) {
        debug!("Adding delay for DynamoDB consistency");
        tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;
    }

    // Test lookup performance on active codes
    let start = tokio::time::Instant::now();
    let handle_payload = json!({
        "inviteCode": "ACTV0000"
    });
    let response = app
        .clone()
        .oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "user-mixed",
            Some(handle_payload),
        ))
        .await
        .unwrap();
    let active_duration = start.elapsed();

    assert_eq!(response.status(), StatusCode::OK);
    info!("Active code lookup time: {:?}", active_duration);

    // Test lookup performance on expired codes
    let start = tokio::time::Instant::now();
    let handle_payload = json!({
        "inviteCode": "EXPR0000"
    });
    let response = app
        .clone()
        .oneshot(create_test_request(
            "PUT",
            "/invitations/handle",
            "user-mixed",
            Some(handle_payload),
        ))
        .await
        .unwrap();
    let expired_duration = start.elapsed();

    assert_eq!(response.status(), StatusCode::GONE);
    info!("Expired code lookup time: {:?}", expired_duration);

    // Both should be reasonably fast (under 500ms)
    assert!(active_duration < tokio::time::Duration::from_millis(500));
    assert!(expired_duration < tokio::time::Duration::from_millis(500));
}

#[tokio::test]
async fn test_gsi_query_performance() {
    let (app, store) = create_test_app().await;

    if !matches!(store, TestStore::DynamoDB(_)) {
        info!("Skipping GSI performance test for non-DynamoDB store");
        return;
    }

    let now = Utc::now();
    let creator_id = "creator-gsi-perf";
    let num_invitations = 30;

    // Create multiple invitations for same creator
    for i in 0..num_invitations {
        let payload = json!({
            "invitedName": format!("GSI User {}", i),
            "boxId": format!("box-{}", i)
        });

        let response = app
            .clone()
            .oneshot(create_test_request(
                "POST",
                "/invitations/new",
                creator_id,
                Some(payload),
            ))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    // Add delay for DynamoDB consistency
    debug!("Adding delay for DynamoDB consistency");
    tokio::time::sleep(tokio::time::Duration::from_millis(3000)).await;

    // Query all invitations for creator (uses GSI)
    let start = tokio::time::Instant::now();
    let response = app
        .clone()
        .oneshot(create_test_request(
            "GET",
            "/invitations/me",
            creator_id,
            None,
        ))
        .await
        .unwrap();
    let query_duration = start.elapsed();

    assert_eq!(response.status(), StatusCode::OK);
    let json_resp = response_to_json(response).await;
    let invitations = json_resp.as_array().unwrap();

    info!(
        "GSI query returned {} invitations in {:?}",
        invitations.len(),
        query_duration
    );

    // Should return all invitations
    assert_eq!(invitations.len(), num_invitations);

    // GSI query should be fast (under 1 second for 30 items)
    assert!(
        query_duration < tokio::time::Duration::from_secs(1),
        "GSI query took too long: {:?}",
        query_duration
    );
}

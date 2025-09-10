use aws_sdk_sns::Client as SnsClient;
use axum::{
    extract::{Extension, Path, State},
    Json,
};
use chrono::{Duration, Utc};
use log::{debug, error, info};
use serde_json::json;
use std::env;
use std::sync::Arc;
use uuid::Uuid;

use lockbox_shared::{models::Invitation, store::InvitationStore};

use crate::{
    error::{map_dynamo_error, AppError, Result},
    models::{ConnectToUserRequest, CreateInvitationRequest, MessageResponse},
};

// Alphabet for user-friendly invitation codes (uppercase letters only)
const CODE_ALPHABET: [char; 26] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
];

// POST /invitations/new - Create a new invitation
pub async fn create_invitation<S: InvitationStore + ?Sized>(
    State(store): State<Arc<S>>,
    Extension(user_id): Extension<String>,
    Json(create_request): Json<CreateInvitationRequest>,
) -> Result<Json<Invitation>> {
    // Generate a user-friendly code for the invitation (8 characters)
    let invite_code = nanoid::nanoid!(8, &CODE_ALPHABET);

    // Set expiration to 48 hours from now
    let created_at = Utc::now().to_rfc3339();
    let expires_at = (Utc::now() + Duration::hours(48)).to_rfc3339();

    // Create the invitation
    let invitation = Invitation {
        id: Uuid::new_v4().to_string(),
        invite_code,
        invited_name: create_request.invited_name,
        box_id: create_request.box_id,
        created_at,
        expires_at,
        opened: false,
        linked_user_id: None,
        creator_id: user_id,
    };

    // Save to database
    let saved_invitation = store
        .create_invitation(invitation)
        .await
        .map_err(|e| map_dynamo_error("create_invitation", e))?;

    // Publish event to SNS
    if let Err(err) = publish_invitation_event(&saved_invitation, "invitation_created").await {
        error!("Failed to publish invitation event: {:?}", err);
    }

    // Return the full invitation object
    Ok(Json(saved_invitation))
}

// PUT /invitation/handle - Connect invitation to user
pub async fn handle_invitation<S: InvitationStore + ?Sized>(
    State(store): State<Arc<S>>,
    Extension(auth_user_id): Extension<String>,
    Json(mut request): Json<ConnectToUserRequest>,
) -> Result<Json<MessageResponse>> {
    // Overwrite payload userId with authenticated user
    request.user_id = auth_user_id.clone();
    // Fetch the invitation by code, propagate NotFound and Expired appropriately
    let mut invitation = store.get_invitation_by_code(&request.invite_code).await?;

    // Prevent replay if the invitation has already been opened or linked
    if invitation.opened || invitation.linked_user_id.is_some() {
        return Err(AppError::Forbidden(format!(
            "Invitation with code {} has already been used",
            request.invite_code
        )));
    }

    // Set as opened and connect to authenticated user
    invitation.opened = true;
    invitation.linked_user_id = Some(auth_user_id.clone());

    // Save the updated invitation
    let updated_invitation = store.update_invitation(invitation.clone()).await?;

    // Publish event to SNS
    if let Err(err) = publish_invitation_event(&updated_invitation, "invitation_viewed").await {
        error!("Failed to publish invitation event: {:?}", err);
    }

    // Return response with box_id to help frontend
    let response = MessageResponse {
        message: format!(
            "User successfully bound to invitation for box {}",
            updated_invitation.box_id
        ),
        box_id: Some(updated_invitation.box_id),
    };

    Ok(Json(response))
}

// Helper function to publish an invitation event to SNS
pub async fn publish_invitation_event(invitation: &Invitation, event_type: &str) -> Result<()> {
    debug!(
        "publish_invitation_event called for event_type={}, invitation_id={}",
        event_type, invitation.id
    );

    // Check if we're in test mode
    if let Ok(test_sns) = env::var("TEST_SNS") {
        if test_sns == "true" {
            // Skip actual SNS publishing in test mode
            debug!(
                "Test mode: Skipping SNS publishing for event_type={}, invitation_id={}",
                event_type, invitation.id
            );
            return Ok(());
        }
    }

    // Get SNS topic ARN from environment variable
    let topic_arn =
        env::var("SNS_TOPIC_ARN").map_err(|e| map_dynamo_error("get_sns_topic_arn", e))?;

    // Create SNS client
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;
    let sns_client = SnsClient::new(&config);

    // Call the internal implementation with the client
    publish_invitation_event_with_client(invitation, sns_client, &topic_arn, event_type).await
}

// Internal implementation that can be mocked for testing
pub async fn publish_invitation_event_with_client(
    invitation: &Invitation,
    sns_client: SnsClient,
    topic_arn: &str,
    event_type: &str,
) -> Result<()> {
    // Create the event payload
    let event_payload = json!({
        "event_type": event_type,
        "invitation_id": invitation.id,
        "box_id": invitation.box_id,
        "user_id": invitation.linked_user_id,
        "invite_code": invitation.invite_code,
        "timestamp": Utc::now().to_rfc3339()
    });

    // Convert to string
    let message = serde_json::to_string(&event_payload)
        .map_err(|e| map_dynamo_error("serialize_event_payload", e))?;

    // Create message attribute
    let message_attribute = aws_sdk_sns::types::MessageAttributeValue::builder()
        .data_type("String")
        .string_value(event_type)
        .build()
        .map_err(|e| map_dynamo_error("build_message_attribute", e))?;

    // Add to HashMap for message attributes
    let mut message_attributes = std::collections::HashMap::new();
    message_attributes.insert("eventType".to_string(), message_attribute);

    let mut publish_request = sns_client
        .publish()
        .topic_arn(topic_arn)
        .message(message)
        .subject("Invitation Viewed");

    // Add message attributes with the proper method
    publish_request = publish_request.set_message_attributes(Some(message_attributes));

    // Send the request
    publish_request
        .send()
        .await
        .map_err(|e| map_dynamo_error("publish_to_sns", e))?;

    Ok(())
}

// PATCH /invitations/:inviteId/refresh - Refresh the invitation
pub async fn refresh_invitation<S: InvitationStore + ?Sized>(
    State(store): State<Arc<S>>,
    Extension(user_id): Extension<String>,
    Path(invite_id): Path<String>,
) -> Result<Json<Invitation>> {
    // Directly fetch the invitation by ID
    let mut invitation = store.get_invitation(&invite_id).await?;

    // Verify that the current user is the creator of this invitation
    if invitation.creator_id != user_id {
        return Err(AppError::Forbidden(format!(
            "Invitation {} is not owned by user",
            invite_id
        )));
    }

    // Check if the invitation has already been opened or linked
    if invitation.opened || invitation.linked_user_id.is_some() {
        return Err(AppError::Forbidden(format!(
            "Invitation {} has already been used and cannot be refreshed.",
            invite_id
        )));
    }

    // Generate a new user-friendly invite code (8 characters)
    invitation.invite_code = nanoid::nanoid!(8, &CODE_ALPHABET);

    // Set new expiration date (48 hours from now)
    invitation.expires_at = (Utc::now() + Duration::hours(48)).to_rfc3339();

    // Save the updated invitation
    let updated_invitation = store.update_invitation(invitation).await?;

    // Return the full updated invitation object
    Ok(Json(updated_invitation))
}

// GET /invitations/me - Get all invitations created by the current user
pub async fn get_my_invitations<S: InvitationStore + ?Sized>(
    State(store): State<Arc<S>>,
    Extension(user_id): Extension<String>,
) -> Result<Json<Vec<Invitation>>> {
    info!("get_my_invitations called with user_id: {}", user_id);

    // Fetch all invitations created by this user
    let invitations = store
        .get_invitations_by_creator_id(&user_id)
        .await
        .map_err(|e| map_dynamo_error("get_invitations_by_creator_id", e))?;

    info!(
        "get_my_invitations returning {} invitations for user_id: {}",
        invitations.len(),
        user_id
    );

    Ok(Json(invitations))
}

// GET /invitations/view/:code - View invitation details by code without consuming it
pub async fn view_invitation_by_code<S: InvitationStore + ?Sized>(
    State(store): State<Arc<S>>,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>> {
    info!("view_invitation_by_code called with code: {}", code);

    // Fetch the invitation by code
    let invitation = store.get_invitation_by_code(&code).await?;

    // Check if invitation is expired
    let expires_at = chrono::DateTime::parse_from_rfc3339(&invitation.expires_at)
        .map_err(|e| AppError::InternalServerError(format!("Failed to parse expiration date: {}", e)))?;
    
    if Utc::now() > expires_at {
        return Err(AppError::NotFound(format!(
            "Invitation with code {} has expired",
            code
        )));
    }

    // We need to get box information from the box service
    // For now, return invitation with a note that box data should be fetched separately
    // In a full implementation, we'd either:
    // 1. Make a cross-service call to box-service
    // 2. Store denormalized box data in the invitation
    // 3. Have the client make a separate call to get box details
    
    // Create an enhanced response with invitation data
    let response = json!({
        "id": invitation.id,
        "inviteCode": invitation.invite_code,
        "invitedName": invitation.invited_name,
        "boxId": invitation.box_id,
        "createdAt": invitation.created_at,
        "expiresAt": invitation.expires_at,
        "opened": invitation.opened,
        "linkedUserId": invitation.linked_user_id,
        "creatorId": invitation.creator_id,
        // Note: Box owner name would need to be fetched from box service
        // For now, frontend can use creatorId or fetch box details separately
        "note": "Box details should be fetched using boxId"
    });

    info!(
        "view_invitation_by_code returning invitation for box_id: {}",
        invitation.box_id
    );

    Ok(Json(response))
}

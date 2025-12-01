use aws_sdk_sns::Client as SnsClient;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use lockbox_shared::store::BoxStore;
use log::{debug, error, info};
use serde_json;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::OnceCell;
use uuid::Uuid;

use crate::error::{AppError, Result};
// Import models from shared crate
use lockbox_shared::models::{now_str, BoxRecord, Document, Guardian};
// Import request/response types from local models
use crate::models::{
    BoxResponse, CreateBoxRequest, DocumentUpdateRequest, DocumentUpdateResponse,
    GuardianUpdateRequest, GuardianUpdateResponse, LockBoxRequest, OptionalField, UpdateBoxRequest,
};

// GET /boxes
pub async fn get_boxes<S>(
    State(store): State<Arc<S>>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Get boxes from store
    let boxes = store.get_boxes_by_owner(&user_id).await?;

    let my_boxes: Vec<_> = boxes.into_iter().map(BoxResponse::from).collect();

    Ok(Json(serde_json::json!({ "boxes": my_boxes })))
}

// GET /boxes/guardian/:id/shard
pub async fn fetch_guardian_shard<S>(
    State(store): State<Arc<S>>,
    Path(id): Path<String>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    let mut box_rec = store.get_box(&id).await?;

    if !box_rec.is_locked {
        return Err(AppError::bad_request(
            "Shard fetch is only available for locked boxes.".into(),
        ));
    }

    let guardian_index = box_rec
        .guardians
        .iter()
        .position(|g| g.id == user_id)
        .ok_or_else(|| AppError::unauthorized("You are not a guardian for this box.".into()))?;

    let total_shards = box_rec.guardians.len();
    let shard_threshold = box_rec
        .shard_threshold
        .unwrap_or_else(|| total_shards as u32);

    let guardian = &mut box_rec.guardians[guardian_index];

    if guardian.encrypted_shard.is_none() {
        return Err(AppError::bad_request(
            "Shard already fetched and removed from server storage.".into(),
        ));
    }

    let shard = guardian
        .encrypted_shard
        .clone()
        .ok_or_else(|| AppError::not_found("Shard not available for this guardian.".into()))?;
    let shard_hash = guardian.shard_hash.clone();

    Ok(Json(serde_json::json!({
        "encryptedShard": shard,
        "shardHash": shard_hash,
        "shardFetchedAt": guardian.shard_fetched_at,
        "shardThreshold": shard_threshold,
        "totalShards": total_shards
    })))
}

// PATCH /boxes/guardian/:id/shard/ack
pub async fn acknowledge_guardian_shard<S>(
    State(store): State<Arc<S>>,
    Path(id): Path<String>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    let mut box_rec = store.get_box(&id).await?;

    if !box_rec.is_locked {
        return Err(AppError::bad_request(
            "Shard acknowledgement is only available for locked boxes.".into(),
        ));
    }

    let guardian_index = box_rec
        .guardians
        .iter()
        .position(|g| g.id == user_id)
        .ok_or_else(|| AppError::unauthorized("You are not a guardian for this box.".into()))?;

    let total_shards = box_rec.guardians.len();

    let guardian = &mut box_rec.guardians[guardian_index];

    if guardian.shard_fetched_at.is_some() && guardian.encrypted_shard.is_none() {
        return Ok(Json(serde_json::json!({
            "shardFetchedAt": guardian.shard_fetched_at.clone(),
            "totalShards": total_shards,
            "shardsFetched": box_rec.shards_fetched.unwrap_or(0),
        })));
    }

    if guardian.encrypted_shard.is_none() {
        return Err(AppError::bad_request(
            "Shard not available to acknowledge.".into(),
        ));
    }

    let fetched_at = now_str();
    guardian.shard_fetched_at = Some(fetched_at.clone());
    guardian.encrypted_shard = None;

    let fetched_count = box_rec
        .guardians
        .iter()
        .filter(|g| g.shard_fetched_at.is_some())
        .count();
    box_rec.shards_fetched = Some(fetched_count);
    box_rec.total_shards = Some(total_shards);
    if fetched_count == total_shards {
        box_rec.shards_deleted_at = Some(now_str());
    }

    let _ = store.update_box(box_rec).await?;

    Ok(Json(serde_json::json!({
        "shardFetchedAt": fetched_at,
        "totalShards": total_shards,
        "shardsFetched": fetched_count
    })))
}

// POST /boxes/guardian/:id/shard/accept
// "Accept" the shard - this is a placebo action for UX purposes.
// The shard data is already stored/fetched; this just records user acknowledgment.
pub async fn accept_guardian_shard<S>(
    State(store): State<Arc<S>>,
    Path(id): Path<String>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    let mut box_rec = store.get_box(&id).await?;

    if !box_rec.is_locked {
        return Err(AppError::bad_request(
            "Shard acceptance is only available for locked boxes.".into(),
        ));
    }

    let guardian_index = box_rec
        .guardians
        .iter()
        .position(|g| g.id == user_id)
        .ok_or_else(|| AppError::unauthorized("You are not a guardian for this box.".into()))?;

    let guardian = &mut box_rec.guardians[guardian_index];

    // If already accepted, return current state
    if guardian.shard_accepted_at.is_some() {
        return Ok(Json(serde_json::json!({
            "message": "Shard already accepted",
            "shardAcceptedAt": guardian.shard_accepted_at.clone(),
            "boxId": box_rec.id,
            "boxName": box_rec.name
        })));
    }

    // Mark as accepted
    let accepted_at = now_str();
    guardian.shard_accepted_at = Some(accepted_at.clone());
    box_rec.updated_at = now_str();

    let box_name = box_rec.name.clone();
    let box_id = box_rec.id.clone();

    let _ = store.update_box(box_rec).await?;

    info!(
        "Guardian {} accepted shard for box_id={}",
        user_id, box_id
    );

    Ok(Json(serde_json::json!({
        "message": "Shard accepted successfully",
        "shardAcceptedAt": accepted_at,
        "boxId": box_id,
        "boxName": box_name
    })))
}

// GET /boxes/:id
pub async fn get_box<S>(
    State(store): State<Arc<S>>,
    Path(id): Path<String>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Get box from store
    let box_rec = store.get_box(&id).await?;

    // TODO: Is it safe to check here or should we do filter in the db query?
    if box_rec.owner_id != user_id {
        return Err(AppError::unauthorized(
            "You don't have permission to view this box".into(),
        ));
    }

    // Return full box info for owner
    Ok(Json(serde_json::json!({
        "box": BoxResponse::from(box_rec)
    })))
}

// POST /boxes
pub async fn create_box<S>(
    State(store): State<Arc<S>>,
    Extension(user_id): Extension<String>,
    Json(payload): Json<CreateBoxRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>)>
where
    S: BoxStore,
{
    let now = now_str();
    let new_box = BoxRecord {
        id: Uuid::new_v4().to_string(),
        name: payload.name,
        description: payload.description,
        is_locked: false,
        locked_at: None,
        created_at: now.clone(),
        updated_at: now.clone(),
        owner_id: user_id,
        owner_name: payload.owner_name,
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

    // Create the box in store
    let created_box = store.create_box(new_box).await?;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "box": BoxResponse::from(created_box) })),
    ))
}

// PATCH /boxes/:id
pub async fn update_box<S>(
    State(store): State<Arc<S>>,
    Path(id): Path<String>,
    Extension(user_id): Extension<String>,
    Json(payload): Json<UpdateBoxRequest>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Get the current box from store
    let mut box_rec = store.get_box(&id).await?;

    // Check if the user is the owner
    if box_rec.owner_id != user_id {
        return Err(AppError::unauthorized(
            "You don't have permission to update this box".into(),
        ));
    }

    // Check if box is locked - prevent modifications
    let has_other_updates = payload.name.is_some()
        || payload.description.is_some()
        || payload.unlock_instructions.is_some();

    if box_rec.is_locked && has_other_updates {
        return Err(AppError::bad_request(
            "Cannot modify a locked box. Locked boxes are immutable.".into(),
        ));
    }

    // Update fields if provided
    if let Some(name) = payload.name {
        box_rec.name = name;
    }

    if let Some(description) = payload.description {
        box_rec.description = description;
    }

    // For unlock_instructions, we need to handle both the case of setting it to a value
    // or explicitly clearing it by setting it to None
    if let Some(field) = &payload.unlock_instructions {
        match field {
            OptionalField::Value(val) => box_rec.unlock_instructions = Some(val.clone()),
            OptionalField::Null => box_rec.unlock_instructions = None,
        }
    }

    if let Some(is_locked) = payload.is_locked {
        // Prevent unlocking a locked box
        if box_rec.is_locked && !is_locked {
            return Err(AppError::bad_request(
                "Cannot unlock a locked box. Locked boxes are immutable.".into(),
            ));
        }

        // If locking the box for the first time, set locked_at timestamp
        if is_locked && !box_rec.is_locked {
            box_rec.locked_at = Some(now_str());
        }
        box_rec.is_locked = is_locked;
    }

    // Save the updated box
    let updated_box = store.update_box(box_rec).await?;

    Ok(Json(
        serde_json::json!({ "box": BoxResponse::from(updated_box) }),
    ))
}

// POST /boxes/owned/:id/lock
pub async fn lock_box<S>(
    State(store): State<Arc<S>>,
    Path(id): Path<String>,
    Extension(user_id): Extension<String>,
    Json(payload): Json<LockBoxRequest>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    let mut box_rec = store.get_box(&id).await?;

    if box_rec.owner_id != user_id {
        return Err(AppError::unauthorized(
            "You don't have permission to lock this box".into(),
        ));
    }

    if box_rec.is_locked {
        return Err(AppError::bad_request(
            "Cannot lock an already locked box.".into(),
        ));
    }

    if payload.shards.len() != box_rec.guardians.len() {
        return Err(AppError::bad_request(
            "Shard count must match the number of guardians.".into(),
        ));
    }

    if payload.shard_threshold < 1 || payload.shard_threshold > payload.shards.len() {
        return Err(AppError::bad_request(
            "Shard threshold must be between 1 and the number of guardians.".into(),
        ));
    }

    for guardian in box_rec.guardians.iter_mut() {
        if let Some(shard) = payload.shards.iter().find(|s| s.guardian_id == guardian.id) {
            guardian.encrypted_shard = Some(shard.shard.clone());
            guardian.shard_hash = Some(shard.shard_hash.clone());
            guardian.shard_fetched_at = None;
        } else {
            return Err(AppError::bad_request(format!(
                "Missing shard for guardian {}",
                guardian.id
            )));
        }
    }

    let now = now_str();
    box_rec.is_locked = true;
    box_rec.locked_at = Some(now.clone());
    box_rec.updated_at = now.clone();
    box_rec.shard_threshold = Some(payload.shard_threshold as u32);
    box_rec.total_shards = Some(payload.shards.len());
    box_rec.shards_fetched = Some(0);
    box_rec.shards_deleted_at = None;

    // Capture data for SNS event before consuming box_rec
    let box_id = box_rec.id.clone();
    let box_name = box_rec.name.clone();
    let owner_name = box_rec.owner_name.clone();
    let guardian_ids: Vec<String> = box_rec
        .guardians
        .iter()
        .filter(|g| !g.id.is_empty())
        .map(|g| g.id.clone())
        .collect();

    let updated_box = store.update_box(box_rec).await?;

    // Publish box_locked event to SNS (fire and forget)
    if let Err(e) = publish_box_locked_event(
        &box_id,
        &box_name,
        owner_name.as_deref(),
        &guardian_ids,
        &now,
    )
    .await
    {
        error!("Failed to publish box_locked event: {:?}", e);
    }

    Ok(Json(
        serde_json::json!({ "box": BoxResponse::from(updated_box) }),
    ))
}

// DELETE /boxes/:id
pub async fn delete_box<S>(
    State(store): State<Arc<S>>,
    Path(id): Path<String>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Get the box to check ownership
    let box_rec = store.get_box(&id).await?;

    // Check if the user is the owner
    if box_rec.owner_id != user_id {
        return Err(AppError::unauthorized(
            "You don't have permission to delete this box".into(),
        ));
    }

    // Delete the box
    store.delete_box(&id).await?;

    Ok(Json(
        serde_json::json!({ "message": "Box deleted successfully." }),
    ))
}

// Helper function to update a guardian in a box
// Returns updated box
async fn update_or_add_guardian<S>(
    store: &S,
    box_id: &str,
    owner_id: &str,
    guardian: &Guardian,
) -> Result<BoxRecord>
where
    S: BoxStore,
{
    // Get the current box from store
    let mut box_rec = store.get_box(box_id).await?;

    // Check if the user is the owner
    if box_rec.owner_id != owner_id {
        return Err(AppError::unauthorized(
            "You don't have permission to update this box".into(),
        ));
    }

    // Check if box is locked
    if box_rec.is_locked {
        return Err(AppError::bad_request(
            "Cannot modify guardians of a locked box. Locked boxes are immutable.".into(),
        ));
    }

    // Check if the guardian already exists in the box
    let guardian_index = box_rec.guardians.iter().position(|g| g.id == guardian.id);

    if let Some(index) = guardian_index {
        // Update existing guardian
        box_rec.guardians[index] = guardian.clone();
    } else {
        // Add new guardian
        box_rec.guardians.push(guardian.clone());
    };

    // Save the updated box
    let updated_box = store.update_box(box_rec).await?;

    Ok(updated_box)
}

// PATCH /boxes/owned/:id/guardian
// This is a dedicated endpoint for updating a single guardian
pub async fn update_guardian<S>(
    State(store): State<Arc<S>>,
    Path(box_id): Path<String>,
    Extension(user_id): Extension<String>,
    Json(payload): Json<GuardianUpdateRequest>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Let the helper function do the work
    let updated_box = update_or_add_guardian(&*store, &box_id, &user_id, &payload.guardian).await?;

    // Find the updated guardian in the updated box
    let updated_guardian = updated_box
        .guardians
        .iter()
        .find(|g| g.id == payload.guardian.id)
        .ok_or_else(|| {
            AppError::internal_server_error("Updated guardian not found in response".into())
        })?;

    // Create a specialized response with the updated guardian and all guardians
    let response = GuardianUpdateResponse {
        id: updated_guardian.id.clone(),
        name: updated_guardian.name.clone(),
        status: updated_guardian.status.to_string(),
        lead_guardian: updated_guardian.lead_guardian,
        added_at: updated_guardian.added_at.clone(),
        invitation_id: updated_guardian.invitation_id.clone(),
        all_guardians: updated_box.guardians.clone(),
        updated_at: updated_box.updated_at.clone(),
    };

    Ok(Json(serde_json::json!({ "guardian": response })))
}

// Helper function to update a document in a box
// Returns updated box
async fn update_or_add_document<S>(
    store: &S,
    box_id: &str,
    owner_id: &str,
    document: &Document,
) -> Result<BoxRecord>
where
    S: BoxStore,
{
    // Get the current box from store
    let mut box_rec = store.get_box(box_id).await?;

    // Check if the user is the owner
    if box_rec.owner_id != owner_id {
        return Err(AppError::unauthorized(
            "You don't have permission to update this box".into(),
        ));
    }

    // Check if box is locked
    if box_rec.is_locked {
        return Err(AppError::bad_request(
            "Cannot modify documents of a locked box. Locked boxes are immutable.".into(),
        ));
    }

    // Check if the document already exists in the box
    let document_index = box_rec.documents.iter().position(|d| d.id == document.id);

    if let Some(index) = document_index {
        // Update existing document
        box_rec.documents[index] = document.clone();
    } else {
        // Add new document
        box_rec.documents.push(document.clone());
    };

    // Save the updated box
    let updated_box = store.update_box(box_rec).await?;

    Ok(updated_box)
}

// PATCH /boxes/owned/:id/document
// This is a dedicated endpoint for updating a single document
pub async fn update_document<S>(
    State(store): State<Arc<S>>,
    Path(box_id): Path<String>,
    Extension(user_id): Extension<String>,
    Json(payload): Json<DocumentUpdateRequest>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Let the helper function do the work
    let updated_box = update_or_add_document(&*store, &box_id, &user_id, &payload.document).await?;

    // Create a specialized response with all documents
    let response = DocumentUpdateResponse {
        documents: updated_box.documents,
        updated_at: updated_box.updated_at,
    };

    Ok(Json(serde_json::json!({ "document": response })))
}

// Helper function to delete a document from a box
// Returns updated box after deletion
async fn delete_document_from_box<S>(
    store: &S,
    box_id: &str,
    owner_id: &str,
    document_id: &str,
) -> Result<BoxRecord>
where
    S: BoxStore,
{
    // Get the current box from store
    let mut box_rec = store.get_box(box_id).await?;

    // Check if the user is the owner
    if box_rec.owner_id != owner_id {
        return Err(AppError::unauthorized(
            "You don't have permission to delete documents from this box".into(),
        ));
    }

    // Check if box is locked
    if box_rec.is_locked {
        return Err(AppError::bad_request(
            "Cannot delete documents from a locked box. Locked boxes are immutable.".into(),
        ));
    }

    // Check if the document exists in the box
    let document_index = box_rec.documents.iter().position(|d| d.id == document_id);

    // Return not found if document doesn't exist
    if document_index.is_none() {
        return Err(AppError::not_found(format!(
            "Document with ID {} not found in box {}",
            document_id, box_id
        )));
    }

    // Remove the document
    box_rec.documents.remove(document_index.unwrap());
    // Save the updated box
    let updated_box = store.update_box(box_rec).await?;

    Ok(updated_box)
}

// DELETE /boxes/owned/:id/document/:document_id
// This is a dedicated endpoint for deleting a single document
pub async fn delete_document<S>(
    State(store): State<Arc<S>>,
    Path((box_id, document_id)): Path<(String, String)>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Use the helper function to delete the document
    let updated_box = delete_document_from_box(&*store, &box_id, &user_id, &document_id).await?;

    // Create a response with all remaining documents
    let response = DocumentUpdateResponse {
        documents: updated_box.documents,
        updated_at: updated_box.updated_at,
    };

    Ok(Json(serde_json::json!({
        "message": "Document deleted successfully",
        "document": response
    })))
}

// Helper function to delete a guardian from a box
// Returns (updated box, deleted guardian) after deletion, with a single read
async fn delete_guardian_from_box<S>(
    store: &S,
    box_id: &str,
    owner_id: &str,
    guardian_id: &str,
) -> Result<(BoxRecord, Guardian)>
where
    S: BoxStore,
{
    // Get the current box from store
    let mut box_rec = store.get_box(box_id).await?;

    // Check if the user is the owner
    if box_rec.owner_id != owner_id {
        return Err(AppError::unauthorized(
            "You don't have permission to delete guardians from this box".into(),
        ));
    }

    // Check if box is locked
    if box_rec.is_locked {
        return Err(AppError::bad_request(
            "Cannot delete guardians from a locked box. Locked boxes are immutable.".into(),
        ));
    }

    // Check if the guardian exists in the box and capture it for response
    // Try to match by guardian.id first, then fall back to invitation_id
    // (guardians in 'invited' status have empty id until they view the invitation)
    let guardian_index = box_rec
        .guardians
        .iter()
        .position(|g| g.id == guardian_id || g.invitation_id == guardian_id);
    let removed_guardian = match guardian_index {
        Some(index) => box_rec.guardians.remove(index),
        None => {
            return Err(AppError::not_found(format!(
                "Guardian with ID or invitation_id {} not found in box {}",
                guardian_id, box_id
            )));
        }
    };
    // Save the updated box
    let updated_box = store.update_box(box_rec).await?;

    Ok((updated_box, removed_guardian))
}

// DELETE /boxes/owned/:id/guardian/:guardian_id
// This is a dedicated endpoint for deleting a single guardian
pub async fn delete_guardian<S>(
    State(store): State<Arc<S>>,
    Path((box_id, guardian_id)): Path<(String, String)>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Use the helper function to delete the guardian (single read)
    let (updated_box, guardian_before) =
        delete_guardian_from_box(&*store, &box_id, &user_id, &guardian_id).await?;

    // Create a response with the deleted guardian info and remaining guardians
    let response = GuardianUpdateResponse {
        id: guardian_before.id,
        name: guardian_before.name,
        status: guardian_before.status.to_string(),
        lead_guardian: guardian_before.lead_guardian,
        added_at: guardian_before.added_at,
        invitation_id: guardian_before.invitation_id,
        all_guardians: updated_box.guardians,
        updated_at: updated_box.updated_at,
    };

    Ok(Json(serde_json::json!({
        "message": "Guardian deleted successfully",
        "guardian": response
    })))
}

// SNS Publishing for box events
static SNS_CLIENT: OnceCell<SnsClient> = OnceCell::const_new();
static TOPIC_ARN: OnceCell<String> = OnceCell::const_new();

/// Publishes a box_locked event to SNS
pub async fn publish_box_locked_event(
    box_id: &str,
    box_name: &str,
    owner_name: Option<&str>,
    guardian_ids: &[String],
    timestamp: &str,
) -> Result<()> {
    debug!(
        "publish_box_locked_event called for box_id={}, guardian_count={}",
        box_id,
        guardian_ids.len()
    );

    // Check if we're in test mode
    if let Ok(test_sns) = env::var("TEST_SNS") {
        if test_sns == "true" {
            debug!(
                "Test mode: Skipping SNS publishing for box_locked event, box_id={}",
                box_id
            );
            return Ok(());
        }
    }

    // Get or initialize SNS client
    let client = SNS_CLIENT
        .get_or_init(|| async {
            let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
                .load()
                .await;
            SnsClient::new(&config)
        })
        .await
        .clone();

    // Get or initialize topic ARN
    let topic_arn = TOPIC_ARN
        .get_or_try_init(|| async {
            env::var("SNS_TOPIC_ARN").map_err(|_| {
                AppError::internal_server_error("SNS_TOPIC_ARN environment variable not set".into())
            })
        })
        .await?;

    // Create the event payload
    let event_payload = serde_json::json!({
        "event_type": "box_locked",
        "box_id": box_id,
        "box_name": box_name,
        "owner_name": owner_name,
        "guardian_ids": guardian_ids,
        "timestamp": timestamp
    });

    let message = serde_json::to_string(&event_payload).map_err(|e| {
        AppError::internal_server_error(format!("Failed to serialize event payload: {}", e))
    })?;

    // Build message attributes for filtering
    let event_type_attr = aws_sdk_sns::types::MessageAttributeValue::builder()
        .data_type("String")
        .string_value("box_locked")
        .build()
        .map_err(|e| {
            AppError::internal_server_error(format!("Failed to build message attribute: {}", e))
        })?;

    let mut message_attributes = HashMap::new();
    message_attributes.insert("eventType".to_string(), event_type_attr);

    // Publish to SNS
    client
        .publish()
        .topic_arn(topic_arn)
        .message(message)
        .subject("Box Locked")
        .set_message_attributes(Some(message_attributes))
        .send()
        .await
        .map_err(|e| {
            AppError::internal_server_error(format!("Failed to publish to SNS: {}", e))
        })?;

    info!(
        "Successfully published box_locked event for box_id={}",
        box_id
    );
    Ok(())
}

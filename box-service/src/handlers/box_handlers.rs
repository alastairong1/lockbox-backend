use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use lockbox_shared::store::BoxStore;
use serde_json;
use std::sync::Arc;
use uuid::Uuid;

use crate::error::{AppError, Result};
// Import models from shared crate
use lockbox_shared::models::{now_str, BoxRecord, Document, Guardian};
// Import request/response types from local models
use crate::models::{
    BoxResponse, CreateBoxRequest, DocumentUpdateRequest, DocumentUpdateResponse,
    GuardianUpdateRequest, GuardianUpdateResponse, OptionalField, UpdateBoxRequest,
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
        created_at: now.clone(),
        updated_at: now.clone(),
        owner_id: user_id,
        owner_name: payload.owner_name,
        documents: vec![],
        guardians: vec![],
        unlock_instructions: None,
        unlock_request: None,
        version: 0,
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
        box_rec.is_locked = is_locked;
    }

    // Save the updated box
    let updated_box = store.update_box(box_rec).await?;

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

    // Check if the guardian exists in the box and capture it for response
    let guardian_index = box_rec.guardians.iter().position(|g| g.id == guardian_id);
    let removed_guardian = match guardian_index {
        Some(index) => box_rec.guardians.remove(index),
        None => {
            return Err(AppError::not_found(format!(
                "Guardian with ID {} not found in box {}",
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

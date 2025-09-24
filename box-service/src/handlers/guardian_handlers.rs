use axum::{
    extract::{Extension, Path, State},
    Json,
};
use log::{debug, trace, warn};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    error::{AppError, Result},
    models::{GuardianInvitationResponse, GuardianResponseRequest, LeadGuardianUpdateRequest},
};

use lockbox_shared::{
    models::{now_str, GuardianStatus, UnlockRequest, UnlockRequestStatus},
    store::{convert_to_guardian_box, BoxStore},
};

// GET /guardianBoxes
pub async fn get_guardian_boxes<S>(
    State(store): State<Arc<S>>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // TODO: For now, we'd need to fetch all boxes and filter on the guardian
    // In a real app, we'd want to add a secondary index in DynamoDB for guardian lookups

    // This is a simplified approach - in production, you would want pagination or a GSI
    let guardian_boxes = store
        .get_boxes_by_guardian_id(&user_id)
        .await
        .unwrap_or_default();

    // Convert BoxRecords to GuardianBox format
    let guardian_boxes: Vec<_> = guardian_boxes
        .iter()
        .filter_map(|b| convert_to_guardian_box(b, &user_id))
        .map(crate::models::GuardianBoxResponse::from)
        .collect();

    Ok(Json(serde_json::json!({ "boxes": guardian_boxes })))
}

// GET /guardianBoxes/:id
pub async fn get_guardian_box<S>(
    State(store): State<Arc<S>>,
    Path(id): Path<String>,
    Extension(user_id): Extension<String>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    trace!("Fetching guardian box with id: {}", id);
    // Fetch the box from store
    let box_rec = store.get_box(&id).await?;
    debug!(
        "Fetched box record for guardian: box_id={}, box_rec={:?}",
        id, box_rec
    );

    // TODO: query DB with filters instead
    if let Some(guardian_box) = convert_to_guardian_box(&box_rec, &user_id) {
        return Ok(Json(
            serde_json::json!({ "box": crate::models::GuardianBoxResponse::from(guardian_box) }),
        ));
    }

    Err(AppError::unauthorized(
        "Unauthorized or Box not found".into(),
    ))
}

// PATCH /boxes/guardian/:id/request - For lead guardian to initiate unlock request
pub async fn request_unlock<S>(
    State(store): State<Arc<S>>,
    Path(box_id): Path<String>,
    Extension(user_id): Extension<String>,
    Json(payload): Json<LeadGuardianUpdateRequest>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Get the box from store
    let mut box_record = store.get_box(&box_id).await?;

    // TODO: query DB with filters instead
    let is_guardian = box_record
        .guardians
        .iter()
        .find(|g| g.id == user_id && g.status != GuardianStatus::Rejected)
        .is_some();

    if !is_guardian {
        warn!("User {} is not a guardian for box {}", user_id, box_id);
        return Err(AppError::unauthorized("Not a guardian for this box".into()));
    }

    // Check if user is a lead guardian by checking the flag in the guardians list
    let is_lead = box_record
        .guardians
        .iter()
        .any(|g| g.id == user_id && g.lead_guardian);

    if is_lead {
        // Lead guardian is initiating an unlock request
        let new_unlock = UnlockRequest {
            id: Uuid::new_v4().to_string(),
            requested_at: now_str(),
            status: UnlockRequestStatus::Requested,
            message: Some(payload.message),
            initiated_by: Some(user_id.clone()),
            approved_by: vec![],
            rejected_by: vec![],
        };

        box_record.unlock_request = Some(new_unlock);
        box_record.updated_at = now_str();

        // Update the box in store
        let updated_box = store.update_box(box_record).await?;

        if let Some(guard_box) = convert_to_guardian_box(&updated_box, &user_id) {
            return Ok(Json(
                serde_json::json!({ "box": crate::models::GuardianBoxResponse::from(guard_box) }),
            ));
        } else {
            return Err(AppError::internal_server_error(
                "Failed to render guardian box".into(),
            ));
        }
    }

    Err(AppError::bad_request(
        "User is not a lead guardian for this box".into(),
    ))
}

// PATCH /boxes/guardian/:id/respond - For guardians to respond to unlock request
pub async fn respond_to_unlock_request<S>(
    State(store): State<Arc<S>>,
    Path(box_id): Path<String>,
    Extension(user_id): Extension<String>,
    Json(payload): Json<GuardianResponseRequest>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Get the box from store
    let mut box_record = store.get_box(&box_id).await?;

    // TODO: query DB with filters instead
    if box_record
        .guardians
        .iter()
        .find(|g| g.id == user_id && g.status != GuardianStatus::Rejected)
        .is_none()
    {
        return Err(AppError::unauthorized("Not a guardian for this box".into()));
    }

    // Check if there's an unlock request to respond to
    if box_record.unlock_request.is_none() {
        return Err(AppError::bad_request(
            "No unlock request exists to update".into(),
        ));
    }

    if let Some(unlock) = &mut box_record.unlock_request {
        let mut updated = false;

        if let Some(approve) = payload.approve {
            if approve && !unlock.approved_by.contains(&user_id) {
                unlock.approved_by.push(user_id.clone());
                updated = true;
            }
        }

        if let Some(reject) = payload.reject {
            if reject && !unlock.rejected_by.contains(&user_id) {
                unlock.rejected_by.push(user_id.clone());
                updated = true;
            }
        }

        if !updated {
            return Err(AppError::bad_request(
                "No valid update field provided".into(),
            ));
        }
    }

    box_record.updated_at = now_str();

    // Update the box in store
    let updated_box = store.update_box(box_record).await?;

    if let Some(guard_box) = convert_to_guardian_box(&updated_box, &user_id) {
        return Ok(Json(
            serde_json::json!({ "box": crate::models::GuardianBoxResponse::from(guard_box) }),
        ));
    } else {
        return Err(AppError::internal_server_error(
            "Failed to render guardian box".into(),
        ));
    }
}

// PATCH /boxes/guardian/:id/invitation - For accepting/rejecting a guardian invitation
pub async fn respond_to_invitation<S>(
    State(store): State<Arc<S>>,
    Path(box_id): Path<String>,
    Extension(user_id): Extension<String>,
    Json(payload): Json<GuardianInvitationResponse>,
) -> Result<Json<serde_json::Value>>
where
    S: BoxStore,
{
    // Get the box from store
    let mut box_record = store.get_box(&box_id).await?;

    // Find if user is a guardian with a pending invitation
    // Pending can be either Invited (not opened) or Viewed (opened/linked)
    let guardian_index = box_record.guardians.iter().position(|g| {
        g.id == user_id
            && (g.status == GuardianStatus::Invited || g.status == GuardianStatus::Viewed)
    });

    if let Some(index) = guardian_index {
        // Update the guardian status based on the acceptance
        if payload.accept {
            box_record.guardians[index].status = GuardianStatus::Accepted;
            box_record.updated_at = now_str();

            // Update the box in store
            let updated_box = store.update_box(box_record).await?;

            if let Some(guard_box) = convert_to_guardian_box(&updated_box, &user_id) {
                return Ok(Json(serde_json::json!({
                    "message": "Guardian invitation accepted successfully",
                    "box": crate::models::GuardianBoxResponse::from(guard_box)
                })));
            } else {
                return Err(AppError::internal_server_error(
                    "Failed to render guardian box".into(),
                ));
            }
        } else {
            // User is rejecting the invitation
            box_record.guardians[index].status = GuardianStatus::Rejected;
            box_record.updated_at = now_str();

            // Update the box in store
            let _updated_box = store.update_box(box_record).await?;

            return Ok(Json(serde_json::json!({
                "message": "Guardian invitation rejected successfully"
            })));
        }
    }

    Err(AppError::bad_request(
        "No pending invitation found for this user".into(),
    ))
}

// Import shared models and store
use lockbox_shared::models::events::InvitationEvent;
use lockbox_shared::models::GuardianStatus;
use lockbox_shared::store::BoxStore;
use std::sync::Arc; // Add Arc for shared state

use log::{error, info, warn}; // Add log import

// Import our custom error type
use crate::errors::AppError;
use crate::errors::InvitationEventError; // Add AppError import

type SharedBoxStore = Arc<dyn BoxStore + Send + Sync>;

// Handler for invitation_created events
// Note: We use the invite_code as a placeholder for the guardian name
// The actual name isn't critical here since it's just for display to the owner
pub async fn handle_invitation_created(
    state: SharedBoxStore,
    event: &InvitationEvent,
) -> Result<(), AppError> {
    info!(
        "Processing invitation_created event for box_id={}, invitation_id={}",
        event.box_id, event.invitation_id
    );

    // Get the box
    let mut box_record = state.get_box(&event.box_id).await.map_err(|e| {
        error!("Failed to get box {}: {}", event.box_id, e);
        AppError::BoxNotFound(format!("Box not found: {}", event.box_id))
    })?;

    // Check if a guardian with this invitation_id already exists
    let guardian_exists = box_record
        .guardians
        .iter()
        .any(|g| g.invitation_id == event.invitation_id);

    if guardian_exists {
        info!(
            "Guardian already exists for invitation_id={}, skipping creation",
            event.invitation_id
        );
        return Ok(());
    }

    // Create the guardian record with 'Invited' status
    // Use the invited_name from the event, or fall back to invite_code
    let guardian_name = event
        .invited_name
        .clone()
        .unwrap_or_else(|| format!("Pending ({})", &event.invite_code));

    let guardian = lockbox_shared::models::Guardian {
        id: String::new(), // Empty until invitation is linked to a user
        name: guardian_name,
        lead_guardian: event.is_lead_guardian,
        status: GuardianStatus::Invited,
        added_at: chrono::Utc::now().to_rfc3339(),
        invitation_id: event.invitation_id.clone(),
        lock_data_received_at: None,
        encrypted_shard: None,
        shard_hash: None,
        shard_fetched_at: None,
        shard_accepted_at: None,
    };

    // Add guardian to box
    box_record.guardians.push(guardian);
    box_record.updated_at = chrono::Utc::now().to_rfc3339();

    // Save the updated box
    state.update_box(box_record).await.map_err(|e| {
        error!("Failed to update box with new guardian: {}", e);
        AppError::from(anyhow::anyhow!("Failed to create guardian: {}", e))
    })?;

    info!(
        "Successfully created guardian for invitation: {}",
        event.invitation_id
    );
    Ok(())
}

// Handler for invitation_opened events
pub async fn handle_invitation_opened(
    state: SharedBoxStore,
    event: &InvitationEvent,
) -> Result<(), AppError> {
    info!(
        "Processing invitation_opened event for box_id={}",
        event.box_id
    );

    // We don't need to extract box_id separately since we use event.box_id directly

    // Extract user_id from event
    let user_id = match &event.user_id {
        Some(id) => id,
        None => {
            error!("User ID is missing in the event");
            return Err(InvitationEventError::MissingField("user_id".to_string()).into());
        }
    };

    // Process invitation viewing (connecting user to invitation)
    match process_invitation_viewing(state, &event.box_id, &event.invitation_id, user_id).await {
        Ok(_) => Ok(()),
        Err(app_error) => {
            // Only propagate critical errors, log and ignore others
            // This is important for resilience - if a box doesn't exist or guardian not found,
            // we don't want to crash the handler
            match &app_error {
                AppError::GuardianNotFound(msg) => {
                    warn!("Ignoring event for non-existent guardian: {}", msg);
                    Ok(())
                }
                AppError::BoxNotFound(msg) => {
                    warn!("Ignoring event for non-existent box: {}", msg);
                    Ok(())
                }
                _ => Err(app_error),
            }
        }
    }
}

// Reasonable retry limit
const MAX_RETRIES: usize = 5;

pub async fn process_invitation_viewing(
    store: SharedBoxStore,
    box_id: &str,
    invitation_id: &str,
    user_id: &str,
) -> Result<(), AppError> {
    info!(
        "Processing invitation viewing: box_id={}, invitation_id={}, user_id={}",
        box_id, invitation_id, user_id
    );

    if user_id.is_empty() {
        return Err(AppError::from(anyhow::anyhow!("User ID cannot be empty")));
    }

    // First check if the box exists
    let box_result = store.get_box(box_id).await;
    if let Err(e) = box_result {
        return Err(AppError::BoxNotFound(format!(
            "Box not found: {}, error: {}",
            box_id, e
        )));
    }

    let mut retries = 0;
    let mut last_error = None;

    while retries < MAX_RETRIES {
        match update_specific_guardian(&store, box_id, invitation_id, user_id).await {
            Ok(_) => {
                info!(
                    "Successfully updated guardian for invitation: box_id={}, invitation_id={}, user_id={}",
                    box_id, invitation_id, user_id
                );
                return Ok(());
            }
            Err(err) => {
                retries += 1;
                last_error = Some(err);

                let base_delay_ms = 50 * (1 << retries); // 50, 100, 200, 400, 800
                let jitter = ((retries as f64 * 0.1) * base_delay_ms as f64) as u64;
                let delay_ms = base_delay_ms + fastrand::u64(0..=jitter);

                info!(
                    "Error updating guardian (retry {}/{}): box_id={}, invitation_id={}, waiting {}ms",
                    retries, MAX_RETRIES, box_id, invitation_id, delay_ms
                );

                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                continue;
            }
        }
    }

    // If we reached max retries, log and return the last error
    if let Some(err) = last_error {
        error!(
            "Failed to update guardian after {} retries: box_id={}, invitation_id={}, user_id={}",
            MAX_RETRIES, box_id, invitation_id, user_id
        );

        // Final check - verify current box state to provide better diagnostics
        match store.get_box(box_id).await {
            Ok(box_record) => {
                let guardian = box_record
                    .guardians
                    .iter()
                    .find(|g| g.invitation_id == invitation_id);
                match guardian {
                    Some(g) => {
                        if g.id == user_id && g.status == GuardianStatus::Viewed {
                            info!(
                                "Guardian was actually updated by another process: box_id={}, invitation_id={}, user_id={}",
                                box_id, invitation_id, user_id
                            );
                            return Ok(());
                        }
                        log::error!(
                            "Current guardian state: id={}, status={}, invitation_id={}",
                            g.id,
                            g.status,
                            g.invitation_id
                        );
                    }
                    None => log::error!(
                        "Guardian with invitation_id={} not found in box",
                        invitation_id
                    ),
                }
            }
            Err(e) => log::error!("Failed to retrieve current box state: {}", e),
        }

        Err(AppError::from(err))
    } else {
        Err(AppError::from(anyhow::anyhow!(
            "Failed to update guardian after max retries"
        )))
    }
}

// New approach that updates only the specific guardian by invitation_id
// instead of updating the entire box at once
async fn update_specific_guardian(
    store: &SharedBoxStore,
    box_id: &str,
    invitation_id: &str,
    user_id: &str,
) -> anyhow::Result<()> {
    // Get the current box state
    let mut box_record = store.get_box(box_id).await?;

    // Find the guardian matching the invitation ID
    let guardian_idx = box_record
        .guardians
        .iter()
        .position(|g| g.invitation_id == invitation_id);

    // If no matching guardian found, return a specific error
    if guardian_idx.is_none() {
        return Err(anyhow::anyhow!(AppError::GuardianNotFound(format!(
            "No guardian found with invitation ID: {}",
            invitation_id
        ))));
    }

    let guardian_idx = guardian_idx.unwrap();
    let guardian = &box_record.guardians[guardian_idx];

    // Skip if already updated to viewed status with correct user ID
    if guardian.status == GuardianStatus::Viewed && guardian.id == user_id {
        log::info!(
            "Guardian already updated, skipping: box_id={}, invitation_id={}, user_id={}",
            box_id,
            invitation_id,
            user_id
        );
        return Ok(());
    }

    // Only update if the guardian is still in "invited" state
    if guardian.status == GuardianStatus::Invited {
        // Make a minimal update - only update this one guardian
        let now = chrono::Utc::now().to_rfc3339();
        box_record.guardians[guardian_idx].id = user_id.to_string();
        box_record.guardians[guardian_idx].status = GuardianStatus::Viewed;
        box_record.updated_at = now;

        // Version bump and optimistic‐locking check occur in
        // DynamoBoxStore::update_box (shared/src/store/dynamo.rs),
        // so we pass through the retrieved version here.

        // Update using the store's update_box method
        match store.update_box(box_record).await {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!(
                    "Failed to update guardian: box_id={}, invitation_id={}, error={:?}",
                    box_id,
                    invitation_id,
                    e
                );
                Err(anyhow::anyhow!(e))
            }
        }
    } else {
        // Guardian is already in a different state, log and consider successful
        log::info!(
            "Guardian already in state {}, not updating: box_id={}, invitation_id={}, user_id={}",
            guardian.status,
            box_id,
            invitation_id,
            user_id
        );
        Ok(())
    }
}

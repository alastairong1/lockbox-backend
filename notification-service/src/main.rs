use aws_lambda_events::event::sns::SnsEvent;
use env_logger;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use lockbox_shared::push::send_shard_notification;
use lockbox_shared::store::dynamo::DynamoPushTokenStore;
use lockbox_shared::store::PushTokenStore;
use log::{error, info, warn};
use serde::Deserialize;

mod errors;

/// Event payload for box_locked events
#[derive(Deserialize, Debug)]
struct BoxLockedEvent {
    event_type: String,
    box_id: String,
    box_name: String,
    owner_name: Option<String>,
    guardian_ids: Vec<String>,
    timestamp: String,
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize env_logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Logging initialized with env_logger");
    info!("Starting Notification Service Lambda");

    // Create the PushToken Store
    let push_store = PushTokenStoreWrapper::new().await;

    // Run the Lambda service function
    lambda_runtime::run(service_fn(|event| handler(event, push_store.clone()))).await?;
    Ok(())
}

/// Wrapper to make the store cloneable for Lambda
#[derive(Clone)]
struct PushTokenStoreWrapper {
    inner: std::sync::Arc<DynamoPushTokenStore>,
}

impl PushTokenStoreWrapper {
    async fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(DynamoPushTokenStore::new().await),
        }
    }
}

/// Lambda handler function
async fn handler(
    event: LambdaEvent<SnsEvent>,
    push_store: PushTokenStoreWrapper,
) -> Result<(), Error> {
    let sns_event = event.payload;

    // Process each record (message) in the SNS event
    for record in sns_event.records {
        let message = record.sns;

        info!("Processing SNS message: {:?}", message.message_id);

        // Try to parse the message as a BoxLockedEvent
        match serde_json::from_str::<BoxLockedEvent>(&message.message) {
            Ok(box_event) => {
                if box_event.event_type != "box_locked" {
                    warn!("Unexpected event type: {}", box_event.event_type);
                    continue;
                }

                info!(
                    "Processing box_locked event for box_id={}, guardian_count={}",
                    box_event.box_id,
                    box_event.guardian_ids.len()
                );

                // Handle the box locked event
                if let Err(e) = handle_box_locked(&push_store, &box_event).await {
                    error!(
                        "Failed to handle box_locked event for box_id={}: {:?}",
                        box_event.box_id, e
                    );
                    // Continue processing other records
                }
            }
            Err(e) => {
                error!("Failed to parse SNS message: {}, error: {}", message.message, e);
                // Continue processing remaining records
                continue;
            }
        }
    }

    Ok(())
}

/// Handle a box_locked event by sending push notifications to guardians
async fn handle_box_locked(
    push_store: &PushTokenStoreWrapper,
    event: &BoxLockedEvent,
) -> Result<(), errors::NotificationError> {
    if event.guardian_ids.is_empty() {
        info!("No guardians to notify for box_id={}", event.box_id);
        return Ok(());
    }

    // Look up push tokens for all guardian IDs
    let tokens = push_store
        .inner
        .get_push_tokens(&event.guardian_ids)
        .await
        .map_err(|e| {
            errors::NotificationError::TokenLookupFailed(format!(
                "Failed to get push tokens: {:?}",
                e
            ))
        })?;

    if tokens.is_empty() {
        info!(
            "No push tokens found for {} guardians of box_id={}",
            event.guardian_ids.len(),
            event.box_id
        );
        return Ok(());
    }

    info!(
        "Found {} push tokens for {} guardians, sending notifications",
        tokens.len(),
        event.guardian_ids.len()
    );

    // Send push notifications
    let owner_name = event.owner_name.as_deref().unwrap_or("Someone");

    send_shard_notification(&tokens, &event.box_name, owner_name, &event.box_id)
        .await
        .map_err(|e| errors::NotificationError::SendFailed(e))?;

    info!(
        "Successfully sent notifications to {} guardians for box_id={}",
        tokens.len(),
        event.box_id
    );

    Ok(())
}

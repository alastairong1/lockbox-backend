use aws_lambda_events::event::cloudwatch_events::CloudWatchEvent;
use chrono::{DateTime, Utc};
use env_logger;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use lockbox_shared::models::BoxRecord;
use lockbox_shared::push::send_shard_reminder_notification;
use lockbox_shared::store::dynamo::{DynamoBoxStore, DynamoPushTokenStore};
use lockbox_shared::store::{BoxStore, PushTokenStore};
use log::{error, info, warn};
use std::sync::Arc;

/// Reminder intervals in hours
const REMINDER_1_HOURS: i64 = 24;
const REMINDER_2_HOURS: i64 = 72;
const REMINDER_3_HOURS: i64 = 168; // 1 week

/// Grace period before first reminder (give user time to see initial notification)
const GRACE_PERIOD_HOURS: i64 = 1;

#[tokio::main]
async fn main() -> Result<(), Error> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting Reminder Service Lambda");

    let box_store = Arc::new(DynamoBoxStore::new().await);
    let push_store = Arc::new(DynamoPushTokenStore::new().await);

    lambda_runtime::run(service_fn(|event| {
        handler(event, box_store.clone(), push_store.clone())
    }))
    .await?;

    Ok(())
}

async fn handler(
    _event: LambdaEvent<CloudWatchEvent>,
    box_store: Arc<DynamoBoxStore>,
    push_store: Arc<DynamoPushTokenStore>,
) -> Result<(), Error> {
    info!("Reminder service triggered");

    let now = Utc::now();

    // Get all locked boxes
    let boxes = match box_store.scan_locked_boxes().await {
        Ok(boxes) => boxes,
        Err(e) => {
            error!("Failed to scan locked boxes: {:?}", e);
            return Err(Error::from(format!("Failed to scan locked boxes: {:?}", e)));
        }
    };

    let box_count = boxes.len();
    info!("Found {} locked boxes to check", box_count);

    let mut reminders_sent = 0;

    for box_rec in &boxes {
        if let Err(e) = process_box(box_rec, &push_store, now).await {
            error!("Failed to process box {}: {:?}", box_rec.id, e);
            // Continue processing other boxes
        } else {
            reminders_sent += 1;
        }
    }

    info!(
        "Reminder service completed. Processed {} boxes, sent reminders for {} guardians",
        box_count, reminders_sent
    );

    Ok(())
}

async fn process_box(
    box_rec: &BoxRecord,
    push_store: &Arc<DynamoPushTokenStore>,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let locked_at = box_rec
        .locked_at
        .as_ref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let locked_at = match locked_at {
        Some(dt) => dt,
        None => {
            warn!(
                "Box {} is locked but has no locked_at timestamp",
                box_rec.id
            );
            return Ok(());
        }
    };

    let owner_name = box_rec.owner_name.as_deref().unwrap_or("Someone");

    for guardian in &box_rec.guardians {
        // Skip if already accepted
        if guardian.shard_accepted_at.is_some() {
            continue;
        }

        // Use lock_data_received_at if available, otherwise fall back to locked_at
        let shard_sent_at = guardian
            .lock_data_received_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(locked_at);

        let hours_since_shard = (now - shard_sent_at).num_hours();

        // Determine which reminder to send (if any)
        let reminder_number = determine_reminder_number(hours_since_shard);

        if reminder_number == 0 {
            // No reminder needed yet
            continue;
        }

        info!(
            "Sending reminder {} to guardian {} for box {} (hours since shard: {})",
            reminder_number, guardian.id, box_rec.id, hours_since_shard
        );

        // Get push token for this guardian
        let tokens = push_store
            .get_push_tokens(&[guardian.id.clone()])
            .await
            .map_err(|e| format!("Failed to get push token: {:?}", e))?;

        if tokens.is_empty() {
            warn!(
                "No push token found for guardian {} of box {}",
                guardian.id, box_rec.id
            );
            continue;
        }

        // Send reminder notification
        if let Err(e) = send_shard_reminder_notification(
            &tokens,
            &box_rec.name,
            owner_name,
            &box_rec.id,
            reminder_number,
        )
        .await
        {
            error!("Failed to send reminder to guardian {}: {}", guardian.id, e);
        } else {
            info!(
                "Successfully sent reminder {} to guardian {}",
                reminder_number, guardian.id
            );
        }
    }

    Ok(())
}

/// Determines which reminder number to send based on hours since shard was sent.
/// Returns 0 if no reminder should be sent (either too early or already past all reminder windows).
///
/// Logic:
/// - Reminder 1: After 24 hours, until 72 hours
/// - Reminder 2: After 72 hours, until 168 hours (1 week)
/// - Reminder 3: After 168 hours (1 week), ongoing
///
/// The function returns the reminder number only during specific windows to avoid
/// sending the same reminder multiple times (service runs every 6 hours).
fn determine_reminder_number(hours_since_shard: i64) -> u32 {
    // Grace period - don't send reminders in the first hour
    if hours_since_shard < GRACE_PERIOD_HOURS {
        return 0;
    }

    // Reminder windows (6 hour windows to account for service running every 6 hours)
    // Reminder 1: 24-30 hours
    if hours_since_shard >= REMINDER_1_HOURS && hours_since_shard < REMINDER_1_HOURS + 6 {
        return 1;
    }

    // Reminder 2: 72-78 hours
    if hours_since_shard >= REMINDER_2_HOURS && hours_since_shard < REMINDER_2_HOURS + 6 {
        return 2;
    }

    // Reminder 3: 168-174 hours (1 week)
    if hours_since_shard >= REMINDER_3_HOURS && hours_since_shard < REMINDER_3_HOURS + 6 {
        return 3;
    }

    // Outside of reminder windows
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_reminder_number() {
        // Too early
        assert_eq!(determine_reminder_number(0), 0);
        assert_eq!(determine_reminder_number(12), 0);
        assert_eq!(determine_reminder_number(23), 0);

        // Reminder 1 window (24-30 hours)
        assert_eq!(determine_reminder_number(24), 1);
        assert_eq!(determine_reminder_number(27), 1);
        assert_eq!(determine_reminder_number(29), 1);

        // Between reminder 1 and 2
        assert_eq!(determine_reminder_number(30), 0);
        assert_eq!(determine_reminder_number(48), 0);
        assert_eq!(determine_reminder_number(71), 0);

        // Reminder 2 window (72-78 hours)
        assert_eq!(determine_reminder_number(72), 2);
        assert_eq!(determine_reminder_number(75), 2);
        assert_eq!(determine_reminder_number(77), 2);

        // Between reminder 2 and 3
        assert_eq!(determine_reminder_number(78), 0);
        assert_eq!(determine_reminder_number(120), 0);
        assert_eq!(determine_reminder_number(167), 0);

        // Reminder 3 window (168-174 hours)
        assert_eq!(determine_reminder_number(168), 3);
        assert_eq!(determine_reminder_number(171), 3);
        assert_eq!(determine_reminder_number(173), 3);

        // After all reminders
        assert_eq!(determine_reminder_number(174), 0);
        assert_eq!(determine_reminder_number(200), 0);
    }
}

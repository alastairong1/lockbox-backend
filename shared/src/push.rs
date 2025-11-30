use log::{error, info};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::models::PushToken;

const EXPO_PUSH_URL: &str = "https://exp.host/--/api/v2/push/send";

#[derive(Debug, Serialize)]
pub struct ExpoPushMessage {
    pub to: String,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct ExpoPushResponse {
    pub data: Vec<ExpoPushTicket>,
}

#[derive(Debug, Deserialize)]
pub struct ExpoPushTicket {
    pub status: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Sends push notifications to multiple tokens
pub async fn send_push_notifications(
    tokens: &[PushToken],
    title: &str,
    body: &str,
    data: Option<serde_json::Value>,
) -> Result<Vec<ExpoPushTicket>, String> {
    if tokens.is_empty() {
        info!("No push tokens provided, skipping push notification");
        return Ok(Vec::new());
    }

    let messages: Vec<ExpoPushMessage> = tokens
        .iter()
        .map(|token| ExpoPushMessage {
            to: token.push_token.clone(),
            title: title.to_string(),
            body: body.to_string(),
            data: data.clone(),
            sound: Some("default".to_string()),
            badge: Some(1),
        })
        .collect();

    info!(
        "Sending {} push notifications to Expo",
        messages.len()
    );

    let client = Client::new();
    let response = client
        .post(EXPO_PUSH_URL)
        .header("Accept", "application/json")
        .header("Accept-Encoding", "gzip, deflate")
        .header("Content-Type", "application/json")
        .json(&messages)
        .send()
        .await
        .map_err(|e| {
            error!("Failed to send push notifications: {}", e);
            format!("Failed to send push notifications: {}", e)
        })?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response.text().await.unwrap_or_default();
        error!(
            "Expo push API returned error status {}: {}",
            status, error_text
        );
        return Err(format!("Expo push API error: {} - {}", status, error_text));
    }

    let push_response: ExpoPushResponse = response.json().await.map_err(|e| {
        error!("Failed to parse Expo push response: {}", e);
        format!("Failed to parse push response: {}", e)
    })?;

    info!(
        "Successfully sent push notifications, got {} tickets",
        push_response.data.len()
    );

    for (i, ticket) in push_response.data.iter().enumerate() {
        if ticket.status != "ok" {
            error!(
                "Push notification {} failed: status={}, message={:?}",
                i, ticket.status, ticket.message
            );
        }
    }

    Ok(push_response.data)
}

/// Sends a shard delivery notification to guardians
pub async fn send_shard_notification(
    tokens: &[PushToken],
    box_name: &str,
    owner_name: &str,
    box_id: &str,
) -> Result<Vec<ExpoPushTicket>, String> {
    let title = "New Key Shard Received";
    let body = format!(
        "{} has locked their Lockbox \"{}\". You've been entrusted with a key shard.",
        owner_name, box_name
    );

    let data = serde_json::json!({
        "type": "shard_received",
        "boxId": box_id,
        "boxName": box_name,
        "ownerName": owner_name
    });

    send_push_notifications(tokens, title, &body, Some(data)).await
}

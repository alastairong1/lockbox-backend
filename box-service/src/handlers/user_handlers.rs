use axum::{Extension, Json};
use lockbox_shared::models::{now_str, PushToken};
use lockbox_shared::store::dynamo::DynamoPushTokenStore;
use lockbox_shared::store::PushTokenStore;
use log::info;
use serde::Deserialize;

use crate::error::{AppError, Result};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPushTokenRequest {
    pub push_token: String,
    pub platform: String,
}

/// PUT /users/push-token
/// Register or update a user's push notification token
pub async fn register_push_token(
    Extension(user_id): Extension<String>,
    Json(request): Json<RegisterPushTokenRequest>,
) -> Result<Json<serde_json::Value>> {
    info!(
        "Registering push token for user: {}, platform: {}",
        user_id, request.platform
    );

    // Validate platform
    if request.platform != "ios" && request.platform != "android" {
        return Err(AppError::bad_request(format!(
            "Invalid platform: {}. Must be 'ios' or 'android'",
            request.platform
        )));
    }

    // Validate push token format (Expo push tokens start with "ExponentPushToken[")
    if !request.push_token.starts_with("ExponentPushToken[") {
        return Err(AppError::bad_request(
            "Invalid push token format. Expected Expo push token.".to_string(),
        ));
    }

    // Create the push token store
    let store = DynamoPushTokenStore::new().await;

    // Create the push token record
    let token = PushToken {
        user_id: user_id.clone(),
        push_token: request.push_token,
        platform: request.platform,
        updated_at: now_str(),
    };

    // Save the token
    store.save_push_token(token).await?;

    info!("Successfully registered push token for user: {}", user_id);

    Ok(Json(serde_json::json!({
        "message": "Push token registered successfully"
    })))
}

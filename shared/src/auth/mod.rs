use axum::{extract::Request, middleware::Next, response::Response};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

use crate::error::{Result, StoreError};

// JWT claims structure - combines both services' implementations
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    #[serde(
        rename = "email_verified",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub email_verified: Option<bool>,
    pub iss: String,
    #[serde(
        rename = "cognito:username",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub cognito_username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin_jti: Option<String>,
    pub aud: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(rename = "token_use", default, skip_serializing_if = "Option::is_none")]
    pub token_use: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_time: Option<usize>,
    pub exp: usize,
    pub iat: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jti: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

// JWT decoder without verification - used since API Gateway already validated the token
pub fn decode_jwt_payload(token: &str) -> Result<Claims> {
    debug!("Decoding JWT payload");

    // Extract payload (second part of the JWT)
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        warn!("Invalid JWT format: expected 3 parts, got {}", parts.len());
        return Err(StoreError::AuthError("Invalid JWT format".into()));
    }

    debug!("JWT structure verified, decoding payload part");

    // Decode the payload
    let payload_data = match URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(data) => data,
        Err(err) => {
            warn!("Failed to base64 decode JWT payload: {:?}", err);
            return Err(StoreError::AuthError("Could not decode JWT payload".into()));
        }
    };

    // Parse the payload
    match serde_json::from_slice::<Claims>(&payload_data) {
        Ok(claims) => {
            debug!("JWT claims parsed successfully: sub={}", claims.sub);
            Ok(claims)
        }
        Err(err) => {
            warn!("Failed to parse JWT claims: {:?}", err);

            // Try to parse as generic JSON to see what fields are missing
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&payload_data) {
                debug!("Raw JWT payload: {:?}", value);
            }

            Err(StoreError::AuthError("Could not parse JWT claims".into()))
        }
    }
}

// Auth middleware for both services
pub async fn auth_middleware(mut request: Request, next: Next) -> Response {
    // Allow only health checks without authentication
    let path = request.uri().path();
    if path == "/health" {
        return next.run(request).await;
    }

    // Log request details
    info!(
        "Auth middleware: method={:?}, path={:?}, query_params={:?}",
        request.method(),
        request.uri().path(),
        request.uri().query()
    );

    // Extract the JWT from the Authorization header
    let auth_header = match request.headers().get("authorization") {
        Some(header) => header,
        None => {
            warn!("Missing authorization header in request");
            return Response::builder()
                .status(http::StatusCode::UNAUTHORIZED)
                .body(axum::body::Body::from("Missing authorization header"))
                .unwrap();
        }
    };

    // Parse the auth header to get the token
    let bearer_token = match auth_header.to_str() {
        Ok(token) => token,
        Err(err) => {
            warn!("Invalid authorization header format: {:?}", err);
            return Response::builder()
                .status(http::StatusCode::UNAUTHORIZED)
                .body(axum::body::Body::from(
                    "Invalid authorization header format",
                ))
                .unwrap();
        }
    };

    if !bearer_token.starts_with("Bearer ") {
        warn!("Authorization header doesn't start with 'Bearer '");
        return Response::builder()
            .status(http::StatusCode::UNAUTHORIZED)
            .body(axum::body::Body::from(
                "Invalid authorization format. Expected 'Bearer <token>'",
            ))
            .unwrap();
    }

    let token = &bearer_token[7..]; // Skip "Bearer " prefix
    debug!("JWT token length: {}", token.len());

    // Simple decode without verification - box service approach
    // API Gateway already verified the token
    let claims = match decode_jwt_payload(token) {
        Ok(claims) => claims,
        Err(_) => {
            return Response::builder()
                .status(http::StatusCode::UNAUTHORIZED)
                .body(axum::body::Body::from("Could not decode JWT payload"))
                .unwrap();
        }
    };

    let user_id = claims.sub;
    info!("Authenticated user ID: {}", user_id);
    info!(
        "JWT claims: sub={}, email={:?}, aud={}",
        user_id, claims.email, claims.aud
    );

    // Store the user_id in the request extensions for later retrieval
    request.extensions_mut().insert(user_id.clone());
    info!("Stored user_id in request extensions: {}", user_id);

    // Continue to the handler
    debug!("Forwarding authenticated request to handler");
    let response = next.run(request).await;
    info!("Handler response status: {:?}", response.status());

    response
}

// Helper function to get the auth headers for testing
pub fn create_jwt_token(user_id: &str) -> String {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use std::time::{SystemTime, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time is before Unix epoch")
        .as_secs() as usize;

    let exp = now + 3600; // 1 hour in the future

    let claims = Claims {
        sub: user_id.to_string(),
        email_verified: Some(true),
        iss: "https://cognito-idp.eu-west-2.amazonaws.com/eu-west-2_rdkfPgGg4".to_string(),
        cognito_username: Some(user_id.to_string()),
        origin_jti: Some("2961a64b-e7ec-4885-994a-d650cc7a7c2d".to_string()),
        aud: "5pgt5gkfulqs0tkdi279c895gp".to_string(),
        event_id: Some("2096030a-d0cb-480a-9318-6f255408c66c".to_string()),
        token_use: Some("id".to_string()),
        auth_time: Some(now - 100),
        exp,
        iat: now - 100,
        jti: Some("021ba19b-7fce-4bc0-b246-852346c43d4e".to_string()),
        email: Some("test@example.com".to_string()),
    };

    // Create JWT header
    let header = Header::new(Algorithm::HS256);

    // In a real scenario, Cognito would use RS256 with a proper key pair
    // For testing purposes, we use HS256 with a simple secret
    let secret = "test_secret_key_for_jwt_encoding_in_tests";
    let encoding_key = EncodingKey::from_secret(secret.as_bytes());

    // Generate the JWT
    encode(&header, &claims, &encoding_key).expect("Failed to create JWT")
}

/// Helper function to create an authorization header with a bearer token for tests
pub fn create_auth_header(user_id: &str) -> (String, String) {
    let token = create_jwt_token(user_id);
    ("authorization".to_string(), format!("Bearer {}", token))
}

/// Helper function to create a test request with authentication headers
pub fn create_test_request(
    method: &str,
    path: &str,
    user_id: &str,
    body: Option<serde_json::Value>,
) -> http::Request<axum::body::Body> {
    let mut builder = http::Request::builder().method(method).uri(path);

    // Add authorization header with JWT
    let (auth_key, auth_value) = create_auth_header(user_id);
    builder = builder.header(auth_key, auth_value);

    // Add content type if there is a body
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }

    // Build the request with the appropriate body
    match body {
        Some(json_body) => builder
            .body(axum::body::Body::from(json_body.to_string()))
            .unwrap(),
        None => builder.body(axum::body::Body::empty()).unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::Extension,
        http::{Request as HttpRequest, StatusCode},
        response::IntoResponse,
        routing::get,
        Router,
    };
    use tower::util::ServiceExt;

    // Dummy handler to check if user_id extension is present
    async fn check_user_id_handler(Extension(user_id): Extension<String>) -> impl IntoResponse {
        if !user_id.is_empty() {
            StatusCode::OK
        } else {
            StatusCode::INTERNAL_SERVER_ERROR // Should not happen if middleware works
        }
    }

    #[tokio::test]
    async fn test_auth_middleware_jwt_token() {
        // Arrange: Router with middleware
        let app = Router::new()
            .route("/", get(check_user_id_handler))
            .layer(axum::middleware::from_fn(auth_middleware));

        // Generate a JWT token for testing
        let token = create_jwt_token("56a20244-0061-708a-0441-62c42ace7b39");

        // Create request with Authorization header
        let request = HttpRequest::builder()
            .uri("/")
            .header("authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        // Act: Call the middleware
        let response = app.oneshot(request).await.unwrap();

        // Assert: Handler received user_id from JWT
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_middleware_missing_header() {
        // Arrange: Router with middleware
        let app = Router::new()
            .route("/", get(check_user_id_handler))
            .layer(axum::middleware::from_fn(auth_middleware));

        // Create request without Authorization header
        let request = HttpRequest::builder().uri("/").body(Body::empty()).unwrap();

        // Act: Call the middleware
        let response = app.oneshot(request).await.unwrap();

        // Assert: Middleware returns unauthorized
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_invalid_token() {
        // Arrange: Router with middleware
        let app = Router::new()
            .route("/", get(check_user_id_handler))
            .layer(axum::middleware::from_fn(auth_middleware));

        // Create request with invalid Authorization header
        let request = HttpRequest::builder()
            .uri("/")
            .header("authorization", "Bearer invalid.token.format")
            .body(Body::empty())
            .unwrap();

        // Act: Call the middleware
        let response = app.oneshot(request).await.unwrap();

        // Assert: Middleware returns unauthorized
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}

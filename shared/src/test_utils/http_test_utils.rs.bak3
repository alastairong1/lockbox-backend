use axum::body::to_bytes;
use serde_json::Value;

/// Helper function to extract JSON from an Axum response
///
/// This is useful in tests to easily parse and assert on JSON responses.
pub async fn response_to_json(response: axum::response::Response) -> Value {
    let body = response.into_body();
    let bytes = to_bytes(body, usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

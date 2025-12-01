use axum::{
    extract::Request,
    middleware,
    routing::{get, patch, post, put},
    Router,
};
use log::{info, warn};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use crate::handlers::{
    box_handlers::{
        accept_guardian_shard, acknowledge_guardian_shard, create_box, delete_box, delete_document,
        delete_guardian, fetch_guardian_shard, get_box, get_boxes, lock_box, update_box,
        update_document, update_guardian,
    },
    guardian_handlers::{
        get_guardian_box, get_guardian_boxes, request_unlock, respond_to_invitation,
        respond_to_unlock_request,
    },
    user_handlers::register_push_token,
};
use lockbox_shared::store::{dynamo::DynamoBoxStore, BoxStore};

// Import shared auth middleware
use lockbox_shared::auth::auth_middleware;

/// Creates a router with the default store
pub async fn create_router() -> Router {
    info!("Creating router with DynamoDB store");

    // Create the DynamoDB store
    let dynamo_store = Arc::new(DynamoBoxStore::new().await);

    // Check if we should remove the base path prefix
    let remove_base_path = std::env::var("REMOVE_BASE_PATH")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    // If REMOVE_BASE_PATH is set to true, don't add the /Prod prefix
    let prefix = if remove_base_path { "" } else { "/Prod" };
    info!("Using API route prefix: {}", prefix);

    create_router_with_store(dynamo_store, prefix)
}

/// Creates a router with a given store implementation
pub fn create_router_with_store<S>(store: Arc<S>, prefix: &str) -> Router
where
    S: BoxStore + 'static,
{
    info!("Setting up API routes with prefix: '{}'", prefix);

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    info!("CORS configured for all origins, methods and headers");

    // Logging middleware to trace all requests
    async fn logging_middleware(
        req: Request,
        next: axum::middleware::Next,
    ) -> impl axum::response::IntoResponse {
        info!(
            "Router received request: method={}, uri={}",
            req.method(),
            req.uri()
        );
        next.run(req).await
    }

    // Create the box API routes (require store state)
    let box_routes = Router::new()
        .route("/boxes/owned", get(get_boxes).post(create_box))
        .route(
            "/boxes/owned/:id",
            get(get_box).patch(update_box).delete(delete_box),
        )
        .route("/boxes/owned/:id/lock", post(lock_box))
        .route("/boxes/owned/:id/guardian", patch(update_guardian))
        .route(
            "/boxes/owned/:id/guardian/:guardian_id",
            axum::routing::delete(delete_guardian),
        )
        .route("/boxes/owned/:id/document", patch(update_document))
        .route(
            "/boxes/owned/:id/document/:document_id",
            axum::routing::delete(delete_document),
        )
        .route("/boxes/guardian", get(get_guardian_boxes))
        .route("/boxes/guardian/:id", get(get_guardian_box))
        .route("/boxes/guardian/:id/shard", get(fetch_guardian_shard))
        .route(
            "/boxes/guardian/:id/shard/ack",
            patch(acknowledge_guardian_shard),
        )
        .route(
            "/boxes/guardian/:id/shard/accept",
            post(accept_guardian_shard),
        )
        .route("/boxes/guardian/:id/request", patch(request_unlock))
        .route(
            "/boxes/guardian/:id/respond",
            patch(respond_to_unlock_request),
        )
        .route(
            "/boxes/guardian/:id/invitation",
            patch(respond_to_invitation),
        )
        .layer(middleware::from_fn(auth_middleware))
        .with_state(store);

    // Create the user API routes (no store state needed)
    let user_routes = Router::new()
        .route("/users/push-token", put(register_push_token))
        .layer(middleware::from_fn(auth_middleware));

    // Merge all API routes
    let api_routes = box_routes.merge(user_routes);

    // Create the main router
    let router = if prefix.is_empty() {
        // For tests or when no prefix is needed, don't nest the routes
        api_routes
            .layer(cors)
            .layer(middleware::from_fn(logging_middleware))
    } else {
        // For production, nest the routes under the prefix
        Router::new()
            .nest(prefix, api_routes)
            .layer(cors)
            .layer(middleware::from_fn(logging_middleware))
    };

    info!(
        "Router configured with all routes and middleware under prefix: '{}'",
        prefix
    );

    // Add a fallback handler for 404s
    router.fallback(|req: Request| async move {
        warn!("No route matched for: {} {}", req.method(), req.uri());
        (
            axum::http::StatusCode::NOT_FOUND,
            "The requested resource was not found".to_string(),
        )
    })
}

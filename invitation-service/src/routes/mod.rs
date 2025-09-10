use axum::{
    extract::Request,
    middleware,
    routing::{get, patch, post, put},
    Router,
};
use log::{debug, info, warn};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use crate::handlers::invitation_handlers::{
    create_invitation, get_my_invitations, handle_invitation, refresh_invitation,
    view_invitation_by_code,
};
// Import shared auth middleware
use lockbox_shared::auth::auth_middleware;
use lockbox_shared::store::{dynamo::DynamoInvitationStore, InvitationStore};

/// Creates a router with the default store
pub async fn create_router() -> Router {
    info!("Creating router with DynamoDB store");

    // Create the DynamoDB store
    let dynamo_store = Arc::new(DynamoInvitationStore::new().await);

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
    S: InvitationStore + ?Sized + 'static,
{
    info!("Setting up API routes with prefix: {}", prefix);

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    debug!("CORS configured for all origins, methods and headers");

    // Logging middleware to trace all requests
    async fn logging_middleware(
        req: Request,
        next: axum::middleware::Next,
    ) -> impl axum::response::IntoResponse {
        debug!(
            "Router received request: method={}, uri={}",
            req.method(),
            req.uri()
        );

        let response = next.run(req).await;
        info!("Responding with status: {}", response.status());
        response
    }

    // Create the API routes
    // Routes that require authentication
    let auth_routes = Router::new()
        .route("/invitations/new", post(create_invitation))
        .route("/invitations/handle", put(handle_invitation))
        .route("/invitations/:inviteId/refresh", patch(refresh_invitation))
        .route("/invitations/me", get(get_my_invitations))
        .layer(middleware::from_fn(auth_middleware));
    
    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/invitations/view/:code", get(view_invitation_by_code));
    
    // Merge routes with state
    let api_routes = Router::new()
        .merge(public_routes)  // Add public routes first
        .merge(auth_routes)    // Then auth routes
        .with_state(store);

    // Create the main router with the prefix
    let router = Router::new()
        .nest(prefix, api_routes)
        .layer(cors)
        .layer(middleware::from_fn(logging_middleware));

    info!(
        "Router configured with all routes and middleware under prefix: {}",
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

mod error;
mod handlers;
mod models;
mod routes;
#[cfg(test)]
mod tests;

use axum::{body::Body, extract::Request, response::Response, Router};
use env_logger;
use http_body_util::BodyExt;
use lambda_http::{
    run, service_fn, Body as LambdaBody, Error, Request as LambdaRequest,
    Response as LambdaResponse,
};
use log::{debug, error, info, trace};
use std::net::SocketAddr;
use tokio::sync::OnceCell;
use tower::ServiceExt;

// Router instance that will be initialized once
static ROUTER: OnceCell<Router> = OnceCell::const_new();

// The Lambda handler function
async fn function_handler(event: LambdaRequest) -> Result<LambdaResponse<LambdaBody>, Error> {
    info!(
        "Received Lambda request: method={:?}, path={:?}, query_params={:?}",
        event.method(),
        event.uri().path(),
        event.uri().query()
    );

    // Get or initialize the router
    let app = ROUTER
        .get_or_init(|| async { routes::create_router().await })
        .await
        .clone();

    let (parts, body) = event.into_parts();
    let body = match body {
        LambdaBody::Empty => Body::empty(),
        LambdaBody::Text(text) => {
            let body_bytes = text.into_bytes();
            debug!(
                "Request body (text): {}",
                String::from_utf8_lossy(&body_bytes)
            );
            Body::from(body_bytes)
        }
        LambdaBody::Binary(data) => {
            debug!("Request body (binary): {} bytes", data.len());
            Body::from(data)
        }
    };

    let http_request = Request::from_parts(parts, body);
    debug!("Created HTTP request: {:?}", http_request);

    info!("Passing request to Axum router");
    let response = match app.oneshot(http_request).await {
        Ok(response) => {
            info!("Received response from Axum: status={}", response.status());
            response
        }
        Err(err) => {
            error!("Error from Axum router: {:?}", err);
            return Err(Error::from(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Infallible error occurred",
            ))));
        }
    };

    let lambda_response = response_to_lambda(response).await?;
    info!(
        "Returning Lambda response: status={}",
        lambda_response.status()
    );

    Ok(lambda_response)
}

// Convert the Axum response to a format suitable for Lambda
async fn response_to_lambda(response: Response) -> Result<LambdaResponse<LambdaBody>, Error> {
    let (parts, body) = response.into_parts();
    debug!(
        "Converting response: status={}, headers={:?}",
        parts.status, parts.headers
    );

    let bytes = match body.collect().await {
        Ok(collected) => {
            let bytes = collected.to_bytes();
            debug!("Response body size: {} bytes", bytes.len());
            bytes
        }
        Err(err) => {
            error!("Failed to read response body: {:?}", err);
            return Err(Error::from(err));
        }
    };

    let builder = LambdaResponse::builder().status(parts.status);

    let builder_with_headers = parts
        .headers
        .iter()
        .fold(builder, |builder, (name, value)| {
            trace!("Adding response header: {}={:?}", name, value);
            builder.header(name, value)
        });

    let lambda_response = if bytes.is_empty() {
        debug!("Creating empty response body");
        builder_with_headers.body(LambdaBody::Empty)?
    } else {
        match String::from_utf8(bytes.to_vec()) {
            Ok(s) => {
                debug!("Creating text response body");
                builder_with_headers.body(LambdaBody::Text(s))?
            }
            Err(_) => {
                debug!("Creating binary response body: {} bytes", bytes.len());
                builder_with_headers.body(LambdaBody::Binary(bytes.to_vec()))?
            }
        }
    };

    Ok(lambda_response)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize env_logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    info!("Logging initialized with env_logger");

    if let Ok(function_name) = std::env::var("AWS_LAMBDA_FUNCTION_NAME") {
        info!(
            "Running in AWS Lambda environment: {} (version: {})",
            function_name,
            std::env::var("AWS_LAMBDA_FUNCTION_VERSION").unwrap_or_else(|_| "unknown".into())
        );
        run(service_fn(function_handler)).await?;
    } else {
        info!("Starting service in non-Lambda environment");
        let addr = SocketAddr::from(([127, 0, 0, 1], 3001));
        info!("listening on {}", addr);

        let app = routes::create_router().await;
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app.into_make_service()).await?;
    }

    info!("Service finished");
    Ok(())
}

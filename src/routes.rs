use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use axum::response::Html;
use std::sync::Arc;

use crate::handlers::{self, AppState};
use crate::models::*;
use crate::config::Config;

// OpenAPI documentation definition
#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::get_time,
        handlers::get_event_status,
        handlers::get_event_stats,
        handlers::query_events,
        handlers::get_db_stats,
        handlers::query_mints,
        handlers::query_orders,
        handlers::query_user_transactions,
        handlers::query_user_orders,
        handlers::test_ipfs_functionality,
        handlers::query_mint_details,
        handlers::query_kline_data,
    ),
    components(
        schemas(
            ApiResponse<TimeResponse>,
            ApiResponse<EventServiceStatus>,
            ApiResponse<EventStats>,
            TimeResponse,
            TimeQuery,
            EventServiceStatus,
            EventStats,
            handlers::EventQueryParams,
            handlers::MintQueryParams,
            handlers::OrderQueryParams,
            handlers::UserQueryParams,
            handlers::MintDetailsQueryParams,
            handlers::TestIpfsParams,
            handlers::KlineQueryParams,
            crate::services::EventQueryResponse,
            crate::services::MintQueryResponse,
            crate::services::OrderQueryResponse,
            crate::services::OrderData,
            crate::services::UserQueryResponse,
            crate::services::UserTransactionData,
            crate::services::UserOrderQueryResponse,
            crate::services::MintDetailsQueryResponse,
            crate::services::MintDetailData,
            crate::solana::SpinPetEvent,
            crate::solana::TokenCreatedEvent,
            crate::solana::BuySellEvent,
            crate::solana::LongShortEvent,
            crate::solana::ForceLiquidateEvent,
            crate::solana::FullCloseEvent,
            crate::solana::PartialCloseEvent,
        )
    ),
    tags(
        (name = "time", description = "Time-related APIs"),
        (name = "events", description = "Event monitoring APIs"),
        (name = "mints", description = "Mint query APIs"),
        (name = "orders", description = "Order query APIs"),
        (name = "user", description = "User transaction query APIs")
    ),
    info(
        title = "Spin API Service",
        description = "A simple and efficient spinpet chain auxiliary API server with event monitoring functionality",
        version = "1.0.0"
    )
)]
pub struct ApiDoc;

pub fn create_router(config: &Config, app_state: Arc<AppState>) -> Router {
    let app = Router::new()
        // API routes
        .route("/api/time", get(handlers::get_time))
        
        // Event-related routes
        .route("/api/events/status", get(handlers::get_event_status))
        .route("/api/events/stats", get(handlers::get_event_stats))
        
        // Event query routes
        .route("/api/events", get(handlers::query_events))
        .route("/api/events/db-stats", get(handlers::get_db_stats))
        
        // Mint query routes
        .route("/api/mints", get(handlers::query_mints))
        
        // Mint details query route
        .route("/api/details", post(handlers::query_mint_details))
        
        // Order query routes
        .route("/api/mint_orders", get(handlers::query_orders))
        
        // User transaction query routes
        .route("/api/user_event", get(handlers::query_user_transactions))
        
        // User order query routes
        .route("/api/user_orders", get(handlers::query_user_orders))
        
        // Test IPFS functionality
        .route("/api/test-ipfs", post(handlers::test_ipfs_functionality))
        
        // OpenAPI specification
        .route("/api-docs/openapi.json", get(serve_openapi))
        
        // Swagger UI
        .route("/swagger-ui", get(serve_swagger_ui))
        
        // Add application state
        .with_state(app_state);

    // Add middleware
    let app = if config.cors.enabled {
        app.layer(create_cors_layer(&config.cors.allow_origins))
    } else {
        app
    };

    app.layer(TraceLayer::new_for_http())
}

// OpenAPI specification handler
async fn serve_openapi() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}

// Swagger UI handler
async fn serve_swagger_ui() -> Html<String> {
    Html(format!(
        r#"
<!DOCTYPE html>
<html>
<head>
    <title>Spin API Documentation</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.0.0/swagger-ui.css" />
    <style>
        html {{
            box-sizing: border-box;
            overflow: -moz-scrollbars-vertical;
            overflow-y: scroll;
        }}
        *, *:before, *:after {{
            box-sizing: inherit;
        }}
        body {{
            margin:0;
            background: #fafafa;
        }}
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.0.0/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@5.0.0/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = function() {{
            const ui = SwaggerUIBundle({{
                url: '/api-docs/openapi.json',
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIStandalonePreset
                ],
                plugins: [
                    SwaggerUIBundle.plugins.DownloadUrl
                ],
                layout: "StandaloneLayout"
            }});
        }};
    </script>
</body>
</html>
        "#
    ))
}

fn create_cors_layer(allow_origins: &[String]) -> CorsLayer {
    use axum::http::{HeaderName, Method};
    
    if allow_origins.contains(&"*".to_string()) {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([
                Method::GET, 
                Method::POST, 
                Method::PUT, 
                Method::DELETE, 
                Method::OPTIONS,
                Method::HEAD,
                Method::PATCH
            ])
            .allow_headers([
                HeaderName::from_static("content-type"),
                HeaderName::from_static("authorization"),
                HeaderName::from_static("accept"),
                HeaderName::from_static("accept-language"),
                HeaderName::from_static("content-language"),
                HeaderName::from_static("origin"),
                HeaderName::from_static("user-agent"),
                HeaderName::from_static("cache-control"),
                HeaderName::from_static("pragma"),
                HeaderName::from_static("x-requested-with"),
                HeaderName::from_static("access-control-request-method"),
                HeaderName::from_static("access-control-request-headers"),
            ])
            .expose_headers([
                HeaderName::from_static("content-length"),
                HeaderName::from_static("content-type"),
                HeaderName::from_static("access-control-allow-origin"),
            ])
            .allow_credentials(false)
            .max_age(std::time::Duration::from_secs(86400)) // 24 hours
    } else {
        let origins: Vec<_> = allow_origins
            .iter()
            .filter_map(|origin| origin.parse().ok())
            .collect();
        
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([
                Method::GET, 
                Method::POST, 
                Method::PUT, 
                Method::DELETE, 
                Method::OPTIONS,
                Method::HEAD,
                Method::PATCH
            ])
            .allow_headers([
                HeaderName::from_static("content-type"),
                HeaderName::from_static("authorization"),
                HeaderName::from_static("accept"),
                HeaderName::from_static("accept-language"),
                HeaderName::from_static("content-language"),
                HeaderName::from_static("origin"),
                HeaderName::from_static("user-agent"),
                HeaderName::from_static("cache-control"),
                HeaderName::from_static("pragma"),
                HeaderName::from_static("x-requested-with"),
                HeaderName::from_static("access-control-request-method"),
                HeaderName::from_static("access-control-request-headers"),
            ])
            .expose_headers([
                HeaderName::from_static("content-length"),
                HeaderName::from_static("content-type"),
                HeaderName::from_static("access-control-allow-origin"),
            ])
            .allow_credentials(true)
            .max_age(std::time::Duration::from_secs(86400)) // 24 hours
    }
} 
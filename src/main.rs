mod config;
mod handlers;
mod models;
mod routes;
mod utils;
mod solana;
mod services;

use std::env;
use std::sync::Arc;
use tracing::{info, error, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::Config;
use crate::routes::create_router;
use crate::services::EventService;
use crate::handlers::AppState;

#[tokio::main]
async fn main() {
    // Initialize configuration
    let config = match Config::new() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("❌ Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize logging
    let log_level = config.logging.level.parse().unwrap_or(tracing::Level::INFO);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("spin_server={}", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize event service
    let event_service = match EventService::new(&config) {
        Ok(service) => Arc::new(tokio::sync::RwLock::new(service)),
        Err(e) => {
            error!("❌ Failed to initialize event service: {}", e);
            warn!("⚠️ Continuing without event listener enabled");
            // Create a disabled config
            let mut disabled_config = config.clone();
            disabled_config.solana.enable_event_listener = false;
            disabled_config.solana.program_id = "11111111111111111111111111111111".to_string(); // Use a valid program ID
            match EventService::new(&disabled_config) {
                Ok(service) => Arc::new(tokio::sync::RwLock::new(service)),
                Err(fallback_err) => {
                    error!("❌ Unable to create disabled event service: {}", fallback_err);
                    std::process::exit(1);
                }
            }
        }
    };

    // Try to start event listener
    if config.solana.enable_event_listener {
        let mut service = event_service.write().await;
        match service.start().await {
            Ok(_) => {
                info!("✅ Event listener started successfully");
            }
            Err(e) => {
                warn!("⚠️ Failed to start event listener: {}", e);
                warn!("⚠️ Server will continue running without event listener");
            }
        }
    } else {
        info!("ℹ️ Event listener is disabled");
    }

    // Get event storage reference
    let event_storage = {
        let service = event_service.read().await;
        service.get_event_storage()
    };

    // Create application state
    let app_state = Arc::new(AppState {
        event_service: Arc::clone(&event_service),
        event_storage,
    });

    // Create router
    let app = create_router(&config, app_state);

    // Create listener
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("❌ Cannot bind to address {}: {}", addr, e);
            std::process::exit(1);
        }
    };

    // Startup information
    info!("🚀 Spin Server started successfully!");
    info!("📍 Listening on: http://{}", addr);
    info!("📖 API documentation: http://{}/swagger-ui", addr);
    info!("🔧 Environment: {}", env::var("RUST_ENV").unwrap_or_else(|_| "development".to_string()));
    info!("🔗 Solana program ID: {}", config.solana.program_id);
    info!("📋 Available endpoints:");
    info!("  GET  /api/time           - Get current time");
    info!("  GET  /api/events/status  - Get event service status");
    info!("  GET  /api/events/stats   - Get event statistics");
    info!("  GET  /api/events         - Query event data");
    info!("  GET  /api/events/db-stats - Get database statistics");

    info!("  GET  /swagger-ui         - API documentation interface");

    // Start server
    if let Err(e) = axum::serve(listener, app).await {
        error!("❌ Server runtime error: {}", e);
        std::process::exit(1);
    }
}

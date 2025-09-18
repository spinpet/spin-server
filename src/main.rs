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
use crate::services::{EventService, KlineSocketService, KlineEventHandler, StatsEventHandler, 
                     start_connection_cleanup_task, start_performance_monitoring_task, KlineConfig};
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

    // Initialize K线推送服务 (如果启用)
    let (kline_socket_service, socketio_layer) = if config.kline.enable_kline_service {
        info!("🚀 Initializing K-line WebSocket service");
        
        // 创建事件存储
        let event_storage = match crate::services::EventStorage::new(&config) {
            Ok(storage) => Arc::new(storage),
            Err(e) => {
                error!("❌ Failed to create event storage: {}", e);
                std::process::exit(1);
            }
        };
        
        // 创建K线配置
        let kline_config = KlineConfig::from_config(&config.kline);
        
        // 创建K线推送服务
        let (kline_service, layer) = match KlineSocketService::new(Arc::clone(&event_storage), kline_config) {
            Ok((service, layer)) => (Arc::new(service), Some(layer)),
            Err(e) => {
                error!("❌ Failed to create K-line socket service: {}", e);
                std::process::exit(1);
            }
        };
        
        // 设置事件处理器
        kline_service.setup_socket_handlers();
        
        info!("✅ K-line WebSocket service initialized");
        (Some(kline_service), layer)
    } else {
        info!("ℹ️ K-line WebSocket service is disabled");
        (None, None)
    };
    
    // Initialize event service with K-line support
    let event_service = match &kline_socket_service {
        Some(kline_service) => {
            // 创建事件存储
            let event_storage = Arc::clone(&kline_service.event_storage);
            
            // 创建统计事件处理器
            let stats_handler = Arc::new(StatsEventHandler::new(Arc::clone(&event_storage)));
            
            // 创建K线事件处理器
            let kline_handler = Arc::new(KlineEventHandler::new(
                Arc::clone(&stats_handler),
                Arc::clone(kline_service)
            ));
            
            // 使用自定义事件处理器创建事件服务
            match EventService::with_handler(&config, Arc::clone(&kline_handler) as Arc<dyn crate::solana::EventHandler>) {
                Ok(service) => Arc::new(tokio::sync::RwLock::new(service)),
                Err(e) => {
                    error!("❌ Failed to initialize event service with K-line handler: {}", e);
                    warn!("⚠️ Falling back to basic event service");
                    
                    match EventService::new(&config) {
                        Ok(service) => Arc::new(tokio::sync::RwLock::new(service)),
                        Err(fallback_err) => {
                            error!("❌ Unable to create fallback event service: {}", fallback_err);
                            std::process::exit(1);
                        }
                    }
                }
            }
        },
        None => {
            // 创建标准的事件服务
            match EventService::new(&config) {
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

    // Create router with optional SocketIO layer
    let app = if let Some(layer) = socketio_layer {
        create_router(&config, app_state).layer(layer)
    } else {
        create_router(&config, app_state)
    };
    
    // Start K-line service background tasks
    if let Some(kline_service) = &kline_socket_service {
        let subscription_manager = Arc::clone(&kline_service.subscriptions);
        let kline_config = KlineConfig::from_config(&config.kline);
        
        // Start connection cleanup task
        let _cleanup_handle = start_connection_cleanup_task(Arc::clone(&subscription_manager), kline_config.clone()).await;
        
        // Start performance monitoring task
        let _monitoring_handle = start_performance_monitoring_task(Arc::clone(&subscription_manager)).await;
        
        info!("✅ K-line service background tasks started");
    }

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
    
    if config.kline.enable_kline_service {
        info!("📊 K-line WebSocket service:");
        info!("  WS   /socket.io          - Real-time K-line data subscription");
        info!("  Events: subscribe, unsubscribe, history, kline_data");
        info!("  Supported intervals: s1, s30, m5");
    }

    // Start server
    if let Err(e) = axum::serve(listener, app).await {
        error!("❌ Server runtime error: {}", e);
        std::process::exit(1);
    }
}

use axum::{
    extract::{Query, State},
    response::Json as ResponseJson,
};
use chrono::{Local, Utc};
use tracing::info;
use std::sync::Arc;

use crate::models::*;
use crate::services::{EventService, EventStorage, KlineSocketService};

/// Application state
pub struct AppState {
    pub event_service: Arc<tokio::sync::RwLock<EventService>>,
    pub event_storage: Arc<EventStorage>,
    pub kline_service: Option<Arc<KlineSocketService>>,
}

/// Get current time
#[utoipa::path(
    get,
    path = "/api/time",
    params(
        ("format" = Option<String>, Query, description = "Time format string")
    ),
    responses(
        (status = 200, description = "Successfully returned time information", body = ApiResponse<TimeResponse>)
    ),
    tag = "time"
)]
pub async fn get_time(Query(params): Query<TimeQuery>) -> ResponseJson<ApiResponse<TimeResponse>> {
    let now_utc = Utc::now();
    let now_local = Local::now();
    
    let format_str = params.format.as_deref().unwrap_or("%Y-%m-%d %H:%M:%S");
    
    let time_response = TimeResponse {
        timestamp: now_utc.timestamp(),
        utc: now_utc.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        local: now_local.format(&format!("{} {}", format_str, "%Z")).to_string(),
        iso8601: now_utc.to_rfc3339(),
    };
    
    info!("Time request completed: {}", time_response.local);
    ResponseJson(ApiResponse::success(time_response))
}

/// Get event service status
#[utoipa::path(
    get,
    path = "/api/events/status",
    responses(
        (status = 200, description = "Successfully returned event service status", body = ApiResponse<EventServiceStatus>)
    ),
    tag = "events"
)]
pub async fn get_event_status(
    State(state): State<Arc<AppState>>,
) -> ResponseJson<ApiResponse<EventServiceStatus>> {
    let event_service = state.event_service.read().await;
    let status = event_service.get_status().await;
    
    info!("Event service status query: running={}", status.is_running);
    ResponseJson(ApiResponse::success(status))
}

/// Get event statistics
#[utoipa::path(
    get,
    path = "/api/events/stats",
    responses(
        (status = 200, description = "Successfully returned event statistics", body = ApiResponse<EventStats>)
    ),
    tag = "events"
)]
pub async fn get_event_stats(
    State(state): State<Arc<AppState>>,
) -> ResponseJson<ApiResponse<EventStats>> {
    let event_service = state.event_service.read().await;
    let stats = event_service.get_stats().await;
    
    info!("Event statistics query: total_events={}", stats.total);
    ResponseJson(ApiResponse::success(stats))
}

/// Get K-line service status
#[utoipa::path(
    get,
    path = "/api/kline/status",
    responses(
        (status = 200, description = "Successfully returned K-line service status", body = serde_json::Value)
    ),
    tag = "kline"
)]
pub async fn get_kline_status(
    State(state): State<Arc<AppState>>,
) -> ResponseJson<ApiResponse<serde_json::Value>> {
    match &state.kline_service {
        Some(kline_service) => {
            let stats = kline_service.get_service_stats().await;
            let status = serde_json::json!({
                "enabled": true,
                "service_status": "running",
                "stats": stats
            });
            info!("K-line service status query completed");
            ResponseJson(ApiResponse::success(status))
        }
        None => {
            let status = serde_json::json!({
                "enabled": false,
                "service_status": "disabled",
                "message": "K-line service is not enabled"
            });
            ResponseJson(ApiResponse::success(status))
        }
    }
}

pub mod event_handlers;
pub use event_handlers::*; 
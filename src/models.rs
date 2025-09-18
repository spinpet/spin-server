use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
// use chrono::{DateTime, Utc};

// General response structure
#[derive(Serialize, ToSchema)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: T,
    pub message: String,
}

// Time response structure
#[derive(Serialize, ToSchema)]
pub struct TimeResponse {
    pub timestamp: i64,
    pub utc: String,
    pub local: String,
    pub iso8601: String,
}

// Time query parameters
#[derive(Deserialize, ToSchema)]
pub struct TimeQuery {
    pub format: Option<String>,
}

// Kline data structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct KlineData {
    pub time: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub is_final: bool,
    pub update_count: u32,
}

// Kline query parameters
#[derive(Debug, Deserialize, ToSchema)]
pub struct KlineQuery {
    pub mint_account: String,
    pub interval: String, // "s1", "s30", "m5"
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub order_by: Option<String>, // "time_asc" or "time_desc" (default)
}

// Kline query response
#[derive(Debug, Serialize, Default, ToSchema)]
pub struct KlineQueryResponse {
    pub klines: Vec<KlineData>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
    pub has_next: bool,
    pub has_prev: bool,
    pub interval: String,
    pub mint_account: String,
}

// Re-export types from services module
pub use crate::services::{EventServiceStatus, EventStats};

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data,
            message: "Operation successful".to_string(),
        }
    }

    pub fn error(message: &str) -> Self
    where
        T: Default,
    {
        Self {
            success: false,
            data: T::default(),
            message: message.to_string(),
        }
    }
} 
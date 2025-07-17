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
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize};
use std::sync::Arc;
use utoipa::ToSchema;

use crate::models::ApiResponse;
use crate::services::event_storage::{EventQuery, EventQueryResponse, MintQuery, MintQueryResponse, OrderQuery, OrderQueryResponse, UserQuery, UserQueryResponse, MintDetailsQueryResponse};
use crate::handlers::AppState;

/// Event query parameters
#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct EventQueryParams {
    /// Token address
    pub mint: String,
    /// Page number (starts from 1)
    pub page: Option<usize>,
    /// Items per page (maximum 1000)
    pub limit: Option<usize>,
    /// Sort order: "slot_asc" or "slot_desc"
    pub order_by: Option<String>,
}

/// Mint query parameters
#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct MintQueryParams {
    /// Page number (starts from 1) - mainly for compatibility, cursor is preferred for large datasets
    pub page: Option<usize>,
    /// Items per page (maximum 1000)
    pub limit: Option<usize>,
    /// Sort order: "slot_asc" (oldest first) or "slot_desc" (newest first)
    pub sort_by: Option<String>,
    /// Cursor for efficient pagination (returned as next_cursor from previous response)
    pub cursor: Option<String>,
}

/// Order query parameters
#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct OrderQueryParams {
    /// Token address
    pub mint: String,
    /// Order type: "up_orders" (short) or "down_orders" (long)
    #[serde(rename = "type")]
    pub order_type: String,
    /// Page number (starts from 1)
    pub page: Option<usize>,
    /// Items per page (maximum 1000)
    pub limit: Option<usize>,
}

/// User transaction query parameters
#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct UserQueryParams {
    /// User address
    pub user: String,
    /// Token address (optional)
    pub mint: Option<String>,
    /// Page number (starts from 1)
    pub page: Option<usize>,
    /// Items per page (maximum 1000)
    pub limit: Option<usize>,
    /// Sort order: "slot_asc" or "slot_desc"
    pub order_by: Option<String>,
}

/// Mint details query parameters
#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = r#"{"mints": ["2M5dgwGNYHAC3CQVYiriY1DYC4GETDDb3ABWv3qsx3Jr", "3TcTZaiCMhCDF2PM7QBzX2aHFeJqLKJrd9LFGLugkr5x"]}"#)]
pub struct MintDetailsQueryParams {
    /// Token addresses
    pub mints: Vec<String>,
}

/// User order query parameters
#[derive(Debug, Deserialize, ToSchema, utoipa::IntoParams)]
pub struct UserOrderQueryParams {
    /// User address
    pub user: String,
    /// Token address (optional) - for more precise query
    pub mint: Option<String>,
    /// Page number (starts from 1)
    pub page: Option<usize>,
    /// Items per page (maximum 1000)
    pub limit: Option<usize>,
    /// Sort order: "start_time_asc" or "start_time_desc"
    pub order_by: Option<String>,
}

/// Event query API
#[utoipa::path(
    get,
    path = "/api/events",
    params(EventQueryParams),
    responses(
        (status = 200, description = "Query successful", body = EventQueryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tags = ["events"]
)]
pub async fn query_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<EventQueryParams>,
) -> Result<Json<ApiResponse<EventQueryResponse>>, StatusCode> {
    // Validate parameters
    if params.mint.is_empty() {
        return Ok(Json(ApiResponse::error("mint parameter cannot be empty")));
    }

    let limit = params.limit.unwrap_or(50);
    if limit > 1000 {
        return Ok(Json(ApiResponse::error("limit cannot exceed 1000")));
    }

    let page = params.page.unwrap_or(1);
    if page < 1 {
        return Ok(Json(ApiResponse::error("page must be greater than 0")));
    }

    // Build query
    let query = EventQuery {
        mint_account: params.mint,
        page: Some(page),
        limit: Some(limit),
        order_by: params.order_by,
    };

    // Execute query
    match state.event_storage.query_events(query).await {
        Ok(response) => Ok(Json(ApiResponse::success(response))),
        Err(e) => {
            tracing::error!("Failed to query events: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Query all mint information
#[utoipa::path(
    get,
    path = "/api/mints",
    params(MintQueryParams),
    responses(
        (status = 200, description = "Query successful", body = MintQueryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tags = ["mints"]
)]
pub async fn query_mints(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MintQueryParams>,
) -> Result<Json<ApiResponse<MintQueryResponse>>, StatusCode> {
    let limit = params.limit.unwrap_or(50);
    if limit > 1000 {
        return Ok(Json(ApiResponse::error("limit cannot exceed 1000")));
    }

    let page = params.page.unwrap_or(1);
    if page < 1 {
        return Ok(Json(ApiResponse::error("page must be greater than 0")));
    }

    // Validate sort_by parameter
    if let Some(ref sort_by) = params.sort_by {
        if !matches!(sort_by.as_str(), "slot_asc" | "slot_desc") {
            return Ok(Json(ApiResponse::error("sort_by must be 'slot_asc' or 'slot_desc'")));
        }
    }

    // Build query
    let query = MintQuery {
        page: Some(page),
        limit: Some(limit),
        sort_by: params.sort_by,
        cursor: params.cursor,
    };

    // Execute query
    match state.event_storage.query_mints(query).await {
        Ok(response) => Ok(Json(ApiResponse::success(response))),
        Err(e) => {
            tracing::error!("Failed to query mint information: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Query order information
#[utoipa::path(
    get,
    path = "/api/mint_orders",
    params(OrderQueryParams),
    responses(
        (status = 200, description = "Query successful", body = OrderQueryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tags = ["orders"]
)]
pub async fn query_orders(
    State(state): State<Arc<AppState>>,
    Query(params): Query<OrderQueryParams>,
) -> Result<Json<ApiResponse<OrderQueryResponse>>, StatusCode> {
    // Validate parameters
    if params.mint.is_empty() {
        return Ok(Json(ApiResponse::error("mint parameter cannot be empty")));
    }

    if !matches!(params.order_type.as_str(), "up_orders" | "down_orders") {
        return Ok(Json(ApiResponse::error("type parameter must be 'up_orders' or 'down_orders'")));
    }
    
    let limit = params.limit.unwrap_or(50);
    if limit > 1000 {
        return Ok(Json(ApiResponse::error("limit cannot exceed 1000")));
    }

    let page = params.page.unwrap_or(1);
    if page < 1 {
        return Ok(Json(ApiResponse::error("page must be greater than 0")));
    }

    // Build query
    let query = OrderQuery {
        mint_account: params.mint,
        order_type: params.order_type,
        page: Some(page),
        limit: Some(limit),
    };

    // Execute query
    match state.event_storage.query_orders(query).await {
        Ok(response) => Ok(Json(ApiResponse::success(response))),
        Err(e) => {
            tracing::error!("Failed to query order information: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Query user transaction information
#[utoipa::path(
    get,
    path = "/api/user_event",
    params(UserQueryParams),
    responses(
        (status = 200, description = "Query successful", body = UserQueryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tags = ["user"]
)]
pub async fn query_user_transactions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<UserQueryParams>,
) -> Result<Json<ApiResponse<UserQueryResponse>>, StatusCode> {
    // Validate parameters
    if params.user.is_empty() {
        return Ok(Json(ApiResponse::error("user parameter cannot be empty")));
    }

    let limit = params.limit.unwrap_or(50);
    if limit > 1000 {
        return Ok(Json(ApiResponse::error("limit cannot exceed 1000")));
    }

    let page = params.page.unwrap_or(1);
    if page < 1 {
        return Ok(Json(ApiResponse::error("page must be greater than 0")));
    }

    // Build query
    let query = UserQuery {
        user: params.user,
        mint_account: params.mint,
        page: Some(page),
        limit: Some(limit),
        order_by: params.order_by,
    };

    // Execute query
    match state.event_storage.query_user_transactions(query).await {
        Ok(response) => Ok(Json(ApiResponse::success(response))),
        Err(e) => {
            tracing::error!("Failed to query user transaction information: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Query mint details
#[utoipa::path(
    post,
    path = "/api/details",
    request_body = MintDetailsQueryParams,
    responses(
        (status = 200, description = "Query successful", body = MintDetailsQueryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tags = ["mints"]
)]
pub async fn query_mint_details(
    State(state): State<Arc<AppState>>,
    Json(params): Json<MintDetailsQueryParams>,
) -> Result<Json<ApiResponse<MintDetailsQueryResponse>>, StatusCode> {
    // Extract mint accounts from params
    let mut mint_accounts = params.mints;
    
    if mint_accounts.is_empty() {
        return Ok(Json(ApiResponse::error("mints parameter cannot be empty")));
    }

    // Limit to 1000 mint addresses
    if mint_accounts.len() > 1000 {
        tracing::warn!("Too many mint addresses requested: {}, limiting to 1000", mint_accounts.len());
        mint_accounts = mint_accounts[0..1000].to_vec();
    }

    // Build query
    let query = crate::services::MintDetailsQuery {
        mint_accounts,
    };

    // Execute query
    match state.event_storage.query_mint_details(query).await {
        Ok(response) => {
            tracing::info!("Mint details query: found {} mint details", response.total);
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            tracing::error!("Failed to query mint details: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Query user orders
#[utoipa::path(
    get,
    path = "/api/user_orders",
    params(UserOrderQueryParams),
    responses(
        (status = 200, description = "Query successful", body = crate::services::UserOrderQueryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tags = ["user"]
)]
pub async fn query_user_orders(
    State(state): State<Arc<AppState>>,
    Query(params): Query<UserOrderQueryParams>,
) -> Result<Json<ApiResponse<crate::services::UserOrderQueryResponse>>, StatusCode> {
    // Validate parameters
    if params.user.is_empty() {
        return Ok(Json(ApiResponse::error("user parameter cannot be empty")));
    }

    let limit = params.limit.unwrap_or(50);
    if limit > 1000 {
        return Ok(Json(ApiResponse::error("limit cannot exceed 1000")));
    }

    let page = params.page.unwrap_or(1);
    if page < 1 {
        return Ok(Json(ApiResponse::error("page must be greater than 0")));
    }

    // Validate order_by parameter
    if let Some(ref order_by) = params.order_by {
        if !matches!(order_by.as_str(), "start_time_asc" | "start_time_desc") {
            return Ok(Json(ApiResponse::error("order_by must be 'start_time_asc' or 'start_time_desc'")));
        }
    }

    // Build query
    let query = crate::services::UserOrderQuery {
        user: params.user,
        mint_account: params.mint,
        page: Some(page),
        limit: Some(limit),
        order_by: params.order_by,
    };

    // Execute query
    match state.event_storage.query_user_orders(query).await {
        Ok(response) => {
            tracing::info!("User orders query: found {} orders for user {}", response.total, response.user);
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            tracing::error!("Failed to query user orders: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get database statistics
#[utoipa::path(
    get,
    path = "/api/events/stats",
    responses(
        (status = 200, description = "Get successful", body = String),
        (status = 500, description = "Internal server error")
    ),
    tags = ["events"]
)]
pub async fn get_db_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    match state.event_storage.get_stats() {
        Ok(stats) => Ok(Json(ApiResponse::success(stats))),
        Err(e) => {
            tracing::error!("Failed to get database statistics: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
} 
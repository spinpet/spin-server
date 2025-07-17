use std::sync::Arc;
use rocksdb::{DB, Options, IteratorMode, Direction};
use tracing::{info, error, debug};
use serde::{Serialize, Deserialize};
use anyhow::Result;
use serde_with::{serde_as, DisplayFromStr};

use crate::solana::events::*;
use crate::config::DatabaseConfig;

/// Event type constants - used for key generation (2 characters to save space)
pub const EVENT_TYPE_TOKEN_CREATED: &str = "tc";
pub const EVENT_TYPE_BUY_SELL: &str = "bs";
pub const EVENT_TYPE_LONG_SHORT: &str = "ls";
pub const EVENT_TYPE_FORCE_LIQUIDATE: &str = "fl";
pub const EVENT_TYPE_FULL_CLOSE: &str = "fc";
pub const EVENT_TYPE_PARTIAL_CLOSE: &str = "pc";

/// Event storage service
pub struct EventStorage {
    db: Arc<DB>,
}

/// Event query parameters
#[derive(Debug, Serialize, Deserialize)]
pub struct EventQuery {
    pub mint_account: String,
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub order_by: Option<String>, // "slot_asc" or "slot_desc"
}

/// Event query response
#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct EventQueryResponse {
    pub events: Vec<SpinPetEvent>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
    pub has_next: bool,
    pub has_prev: bool,
}

/// Mint query parameters
#[derive(Debug, Serialize, Deserialize)]
pub struct MintQuery {
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

/// Mint information
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MintInfo {
    pub mint_account: String,
}

/// Mint query response
#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct MintQueryResponse {
    pub mints: Vec<String>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
    pub has_next: bool,
    pub has_prev: bool,
}

/// Order data
#[serde_as]
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OrderData {
    pub order_type: u8,
    pub mint: String,
    pub user: String,
    #[serde_as(as = "DisplayFromStr")]
    pub lock_lp_start_price: u128,
    #[serde_as(as = "DisplayFromStr")]
    pub lock_lp_end_price: u128,
    pub lock_lp_sol_amount: u64,
    pub lock_lp_token_amount: u64,
    pub start_time: u32,
    pub end_time: u32,
    pub margin_sol_amount: u64,
    pub borrow_amount: u64,
    pub position_asset_amount: u64,
    pub borrow_fee: u16,
    pub order_pda: String,
}

/// Order query parameters
#[derive(Debug, Serialize, Deserialize)]
pub struct OrderQuery {
    pub mint_account: String,
    pub order_type: String, // "up_orders" or "down_orders"
}

/// Order query response
#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct OrderQueryResponse {
    pub orders: Vec<OrderData>,
    pub total: usize,
    pub order_type: String,
    pub mint_account: String,
}

/// User transaction data
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UserTransactionData {
    pub event_type: String, // "long_short", "force_liquidate", "full_close", "partial_close"
    pub user: String,
    pub mint_account: String,
    pub slot: u64,
    pub timestamp: i64,
    pub signature: String,
    pub event_data: serde_json::Value, // Store complete event data
}

/// User transaction query parameters
#[derive(Debug, Serialize, Deserialize)]
pub struct UserQuery {
    pub user: String,
    pub mint_account: Option<String>,
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub order_by: Option<String>, // "slot_asc" or "slot_desc"
}

/// User transaction query response
#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct UserQueryResponse {
    pub transactions: Vec<UserTransactionData>,
    pub total: usize,
    pub page: usize,
    pub limit: usize,
    pub has_next: bool,
    pub has_prev: bool,
    pub user: String,
    pub mint_account: Option<String>,
}

/// Mint detail information
#[serde_as]
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema, Default)]
pub struct MintDetailData {
    pub mint_account: String,
    pub payer: Option<String>,
    pub curve_account: Option<String>,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub uri: Option<String>,
    #[schema(value_type = String)]
    pub create_timestamp: Option<i64>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub latest_price: Option<u128>,
    #[schema(value_type = String)]
    pub latest_trade_time: Option<i64>,
    pub total_sol_amount: u64,
    pub total_margin_sol_amount: u64,
    pub total_force_liquidations: u64,
    pub total_close_profit: u64,
}

/// Mint details query parameters
#[derive(Debug, Serialize, Deserialize)]
pub struct MintDetailsQuery {
    pub mint_accounts: Vec<String>,
}

/// Mint details query response
#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct MintDetailsQueryResponse {
    pub details: Vec<MintDetailData>,
    pub total: usize,
}

impl EventStorage {
    /// Create a new event storage instance
    pub fn new(config: &DatabaseConfig) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_write_buffer_size(64 * 1024 * 1024); // 64MB
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base(64 * 1024 * 1024); // 64MB
        opts.set_level_zero_file_num_compaction_trigger(8);
        opts.set_level_zero_slowdown_writes_trigger(17);
        opts.set_level_zero_stop_writes_trigger(24);
        opts.set_num_levels(7);
        opts.set_max_bytes_for_level_base(256 * 1024 * 1024); // 256MB
        opts.set_max_bytes_for_level_multiplier(10.0);
        
        let db = DB::open(&opts, &config.rocksdb_path)?;
        
        info!("🗄️ RocksDB initialized successfully, path: {}", config.rocksdb_path);
        Ok(Self {
            db: Arc::new(db),
        })
    }

    /// Generate event storage key
    /// Format: tr:{mint_account}:{slot(10 digits with leading zeros)}:{event_type}:{signature}
    fn generate_event_key(&self, event: &SpinPetEvent) -> String {
        let (mint_account, slot, signature, event_type) = match event {
            SpinPetEvent::TokenCreated(e) => (
                &e.mint_account,
                e.slot,
                &e.signature,
                EVENT_TYPE_TOKEN_CREATED
            ),
            SpinPetEvent::BuySell(e) => (
                &e.mint_account,
                e.slot,
                &e.signature,
                EVENT_TYPE_BUY_SELL
            ),
            SpinPetEvent::LongShort(e) => (
                &e.mint_account,
                e.slot,
                &e.signature,
                EVENT_TYPE_LONG_SHORT
            ),
            SpinPetEvent::ForceLiquidate(e) => (
                &e.mint_account,
                e.slot,
                &e.signature,
                EVENT_TYPE_FORCE_LIQUIDATE
            ),
            SpinPetEvent::FullClose(e) => (
                &e.mint_account,
                e.slot,
                &e.signature,
                EVENT_TYPE_FULL_CLOSE
            ),
            SpinPetEvent::PartialClose(e) => (
                &e.mint_account,
                e.slot,
                &e.signature,
                EVENT_TYPE_PARTIAL_CLOSE
            ),
        };
        
        // Format slot as 10 digits with leading zeros, for correct sorting by dictionary order
        format!("tr:{}:{:010}:{}:{}", mint_account, slot, event_type, signature)
    }

    /// Generate mint marker key
    /// Format: mt:{mint_account}
    fn generate_mint_key(&self, event: &SpinPetEvent) -> String {
        let mint_account = match event {
            SpinPetEvent::TokenCreated(e) => &e.mint_account,
            SpinPetEvent::BuySell(e) => &e.mint_account,
            SpinPetEvent::LongShort(e) => &e.mint_account,
            SpinPetEvent::ForceLiquidate(e) => &e.mint_account,
            SpinPetEvent::FullClose(e) => &e.mint_account,
            SpinPetEvent::PartialClose(e) => &e.mint_account,
        };
        
        format!("mt:{}", mint_account)
    }

    /// Generate order key
    /// Format: or:{mint_account}:up:{order_pda} or or:{mint_account}:dn:{order_pda}
    fn generate_order_key(&self, mint_account: &str, order_type: u8, order_pda: &str) -> String {
        let type_str = if order_type == 2 { "up" } else { "dn" };
        format!("or:{}:{}:{}", mint_account, type_str, order_pda)
    }

    /// Generate user transaction key
    /// Format: us:{user}:{mint_account}:{slot}
    fn generate_user_transaction_key(&self, user: &str, mint_account: &str, slot: u64) -> String {
        format!("us:{}:{}:{:010}", user, mint_account, slot)
    }

    /// Create OrderData from LongShortEvent
    fn create_order_data_from_long_short(&self, event: &LongShortEvent) -> OrderData {
        OrderData {
            order_type: event.order_type,
            mint: event.mint.clone(),
            user: event.user.clone(),
            lock_lp_start_price: event.lock_lp_start_price,
            lock_lp_end_price: event.lock_lp_end_price,
            lock_lp_sol_amount: event.lock_lp_sol_amount,
            lock_lp_token_amount: event.lock_lp_token_amount,
            start_time: event.start_time,
            end_time: event.end_time,
            margin_sol_amount: event.margin_sol_amount,
            borrow_amount: event.borrow_amount,
            position_asset_amount: event.position_asset_amount,
            borrow_fee: event.borrow_fee,
            order_pda: event.order_pda.clone(),
        }
    }

    /// Create OrderData from PartialCloseEvent
    fn create_order_data_from_partial_close(&self, event: &PartialCloseEvent) -> OrderData {
        OrderData {
            order_type: event.order_type,
            mint: event.mint.clone(),
            user: event.user.clone(),
            lock_lp_start_price: event.lock_lp_start_price,
            lock_lp_end_price: event.lock_lp_end_price,
            lock_lp_sol_amount: event.lock_lp_sol_amount,
            lock_lp_token_amount: event.lock_lp_token_amount,
            start_time: event.start_time,
            end_time: event.end_time,
            margin_sol_amount: event.margin_sol_amount,
            borrow_amount: event.borrow_amount,
            position_asset_amount: event.position_asset_amount,
            borrow_fee: event.borrow_fee,
            order_pda: event.order_pda.clone(),
        }
    }

    /// Create user transaction data
    fn create_user_transaction_data(&self, event: &SpinPetEvent) -> Option<UserTransactionData> {
        match event {
            SpinPetEvent::LongShort(e) => {
                Some(UserTransactionData {
                    event_type: "long_short".to_string(),
                    user: e.user.clone(),
                    mint_account: e.mint_account.clone(),
                    slot: e.slot,
                    timestamp: e.timestamp.timestamp(),
                    signature: e.signature.clone(),
                    event_data: serde_json::to_value(e).unwrap_or(serde_json::Value::Null),
                })
            }
            SpinPetEvent::ForceLiquidate(e) => {
                // ForceLiquidateEvent doesn't have a user field, we need to get user info from order_pda
                // This requires additional query, for now we'll use payer as user
                Some(UserTransactionData {
                    event_type: "force_liquidate".to_string(),
                    user: e.payer.clone(), // Use payer as user
                    mint_account: e.mint_account.clone(),
                    slot: e.slot,
                    timestamp: e.timestamp.timestamp(),
                    signature: e.signature.clone(),
                    event_data: serde_json::to_value(e).unwrap_or(serde_json::Value::Null),
                })
            }
            SpinPetEvent::FullClose(e) => {
                // FullCloseEvent also doesn't have a clear user field, use payer
                Some(UserTransactionData {
                    event_type: "full_close".to_string(),
                    user: e.payer.clone(),
                    mint_account: e.mint_account.clone(),
                    slot: e.slot,
                    timestamp: e.timestamp.timestamp(),
                    signature: e.signature.clone(),
                    event_data: serde_json::to_value(e).unwrap_or(serde_json::Value::Null),
                })
            }
            SpinPetEvent::PartialClose(e) => {
                Some(UserTransactionData {
                    event_type: "partial_close".to_string(),
                    user: e.user.clone(),
                    mint_account: e.mint_account.clone(),
                    slot: e.slot,
                    timestamp: e.timestamp.timestamp(),
                    signature: e.signature.clone(),
                    event_data: serde_json::to_value(e).unwrap_or(serde_json::Value::Null),
                })
            }
            _ => None,
        }
    }

    /// Generate mint detail key
    /// Format: in:{mint_account}
    fn generate_mint_detail_key(&self, mint_account: &str) -> String {
        format!("in:{}", mint_account)
    }

    /// Process events for mint detail data
    pub async fn process_event_for_mint_detail(&self, event: &SpinPetEvent) -> Result<()> {
        let mint_account = match event {
            SpinPetEvent::TokenCreated(e) => &e.mint_account,
            SpinPetEvent::BuySell(e) => &e.mint_account,
            SpinPetEvent::LongShort(e) => &e.mint_account,
            SpinPetEvent::ForceLiquidate(e) => &e.mint_account,
            SpinPetEvent::FullClose(e) => &e.mint_account,
            SpinPetEvent::PartialClose(e) => &e.mint_account,
        };

        let key = self.generate_mint_detail_key(mint_account);
        let mut detail = match self.db.get(key.as_bytes())? {
            Some(data) => serde_json::from_slice::<MintDetailData>(&data)
                .unwrap_or_else(|_| MintDetailData {
                    mint_account: mint_account.to_string(),
                    ..Default::default()
                }),
            None => MintDetailData {
                mint_account: mint_account.to_string(),
                ..Default::default()
            },
        };

        // Update detail data based on event type
        match event {
            SpinPetEvent::TokenCreated(e) => {
                detail.payer = Some(e.payer.clone());
                detail.curve_account = Some(e.curve_account.clone());
                detail.name = Some(e.name.clone());
                detail.symbol = Some(e.symbol.clone());
                detail.uri = Some(e.uri.clone());
                detail.create_timestamp = Some(e.timestamp.timestamp());
                detail.latest_trade_time = Some(e.timestamp.timestamp());
            }
            SpinPetEvent::BuySell(e) => {
                detail.latest_price = Some(e.latest_price);
                detail.latest_trade_time = Some(e.timestamp.timestamp());
                detail.total_sol_amount = detail.total_sol_amount.saturating_add(e.sol_amount);
            }
            SpinPetEvent::LongShort(e) => {
                detail.latest_price = Some(e.latest_price);
                detail.latest_trade_time = Some(e.timestamp.timestamp());
                detail.total_margin_sol_amount = detail.total_margin_sol_amount.saturating_add(e.margin_sol_amount);
            }
            SpinPetEvent::ForceLiquidate(e) => {
                detail.total_force_liquidations = detail.total_force_liquidations.saturating_add(1);
            }
            SpinPetEvent::FullClose(e) => {
                detail.latest_price = Some(e.latest_price);
                detail.latest_trade_time = Some(e.timestamp.timestamp());
                detail.total_close_profit = detail.total_close_profit.saturating_add(e.user_close_profit);
            }
            SpinPetEvent::PartialClose(e) => {
                detail.latest_price = Some(e.latest_price);
                detail.latest_trade_time = Some(e.timestamp.timestamp());
                detail.total_close_profit = detail.total_close_profit.saturating_add(e.user_close_profit);
            }
        }

        // Save updated detail data
        let value = serde_json::to_vec(&detail)?;
        self.db.put(key.as_bytes(), &value)?;
        debug!("💾 Mint detail updated successfully, key: {}", key);

        Ok(())
    }

    /// Query mint details
    pub async fn query_mint_details(&self, query: MintDetailsQuery) -> Result<MintDetailsQueryResponse> {
        let mut details = Vec::new();

        for mint_account in query.mint_accounts {
            let key = self.generate_mint_detail_key(&mint_account);
            if let Some(data) = self.db.get(key.as_bytes())? {
                match serde_json::from_slice::<MintDetailData>(&data) {
                    Ok(detail) => details.push(detail),
                    Err(e) => {
                        error!("❌ Failed to parse mint detail data: {}, mint: {}", e, mint_account);
                        continue;
                    }
                }
            }
        }

        let total = details.len();
        
        debug!("🔍 Queried {} mint details", total);
        
        Ok(MintDetailsQueryResponse {
            details,
            total,
        })
    }

    /// Store event
    pub async fn store_event(&self, event: SpinPetEvent) -> Result<()> {
        let key = self.generate_event_key(&event);
        let value = serde_json::to_vec(&event)?;
        
        // Also store mint marker (direct overwrite, no check needed)
        let mint_key = self.generate_mint_key(&event);
        
        let mut batch = rocksdb::WriteBatch::default();
        batch.put(key.as_bytes(), &value);
        batch.put(mint_key.as_bytes(), b""); // Empty value marker
        
        // Process order-related events
        match &event {
            SpinPetEvent::LongShort(long_short_event) => {
                // Create order data
                let order_data = self.create_order_data_from_long_short(long_short_event);
                let order_key = self.generate_order_key(
                    &long_short_event.mint_account,
                    long_short_event.order_type,
                    &long_short_event.order_pda
                );
                let order_value = serde_json::to_vec(&order_data)?;
                batch.put(order_key.as_bytes(), &order_value);
                debug!("💾 Order data stored successfully, key: {}", order_key);
            }
            SpinPetEvent::PartialClose(partial_close_event) => {       
                // Update order data
                let order_data = self.create_order_data_from_partial_close(partial_close_event);
                let order_key = self.generate_order_key(
                    &partial_close_event.mint_account,
                    partial_close_event.order_type,
                    &partial_close_event.order_pda
                );
                let order_value = serde_json::to_vec(&order_data)?;
                batch.put(order_key.as_bytes(), &order_value);
                debug!("💾 Order data updated successfully, key: {}", order_key);
            }
            SpinPetEvent::FullClose(full_close_event) => {
                // Delete order data (need to know order_type, get from event)
                // Since FullCloseEvent includes is_close_long field, we can infer order_type
                let order_type = if full_close_event.is_close_long { 1 } else { 2 };
                let order_key = self.generate_order_key(
                    &full_close_event.mint_account,
                    order_type,
                    &full_close_event.order_pda
                );
                batch.delete(order_key.as_bytes());
                debug!("💾 Order data deleted successfully, key: {}", order_key);
            }
            SpinPetEvent::ForceLiquidate(force_liquidate_event) => {
                // Force liquidation: search and delete in both up and dn
                let up_key = self.generate_order_key(
                    &force_liquidate_event.mint_account,
                    2, // up (short)
                    &force_liquidate_event.order_pda
                );
                let dn_key = self.generate_order_key(
                    &force_liquidate_event.mint_account,
                    1, // dn (long)
                    &force_liquidate_event.order_pda
                );
                
                // Check which key exists and delete
                if self.db.get(up_key.as_bytes())?.is_some() {
                    batch.delete(up_key.as_bytes());
                    debug!("💾 Force liquidation order deleted successfully, key: {}", up_key);
                }
                if self.db.get(dn_key.as_bytes())?.is_some() {
                    batch.delete(dn_key.as_bytes());
                    debug!("💾 Force liquidation order deleted successfully, key: {}", dn_key);
                }
            }
                         _ => {
                 // Other event types don't need order processing
             }
         }

         // Process user transaction records
         if let Some(user_transaction) = self.create_user_transaction_data(&event) {
             let user_key = self.generate_user_transaction_key(
                 &user_transaction.user,
                 &user_transaction.mint_account,
                 user_transaction.slot
             );
             let user_value = serde_json::to_vec(&user_transaction)?;
             batch.put(user_key.as_bytes(), &user_value);
             debug!("💾 User transaction recorded successfully, key: {}", user_key);
         }

         // Process mint detail data
         self.process_event_for_mint_detail(&event).await?;
         
         self.db.write(batch)?;
        
        debug!("💾 Event stored successfully, key: {}", key);
        Ok(())
    }

    /// Batch store events
    pub async fn store_events(&self, events: Vec<SpinPetEvent>) -> Result<()> {
        let mut batch = rocksdb::WriteBatch::default();
        
        for event in &events {
            let key = self.generate_event_key(event);
            let value = serde_json::to_vec(event)?;
            batch.put(key.as_bytes(), &value);
            
            // Also store mint marker
            let mint_key = self.generate_mint_key(event);
            batch.put(mint_key.as_bytes(), b""); // Empty value marker
        }
        
        self.db.write(batch)?;
        
        // Process mint detail data for each event
        for event in events {
            if let Err(e) = self.process_event_for_mint_detail(&event).await {
                error!("❌ Failed to process mint detail data for event: {}", e);
                // Continue processing other events
            }
        }
        
        debug!("💾 Batch events stored successfully");
        Ok(())
    }

    /// Query events
    pub async fn query_events(&self, query: EventQuery) -> Result<EventQueryResponse> {
        let mint_account = &query.mint_account;
        let page = query.page.unwrap_or(1);
        let limit = query.limit.unwrap_or(50);
        let order_by = query.order_by.unwrap_or_else(|| "slot_desc".to_string());
        
        // Build prefix key
        let prefix = format!("tr:{}:", mint_account);
        
        debug!("🔍 Querying events, mint: {}, page: {}, limit: {}, order: {}", 
               mint_account, page, limit, order_by);
        
        // Collect all matching events
        let mut all_events = Vec::new();
        
        let iter = self.db.iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));
        
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            
            // Check if still matches prefix
            if !key_str.starts_with(&prefix) {
                break;
            }
            
            // Parse event data
            match serde_json::from_slice::<SpinPetEvent>(&value) {
                Ok(event) => all_events.push(event),
                Err(e) => {
                    error!("❌ Failed to parse event data: {}, key: {}", e, key_str);
                    continue;
                }
            }
        }
        
        // Sort by slot
        match order_by.as_str() {
            "slot_asc" => {
                all_events.sort_by(|a, b| self.get_event_slot(a).cmp(&self.get_event_slot(b)));
            }
            "slot_desc" => {
                all_events.sort_by(|a, b| self.get_event_slot(b).cmp(&self.get_event_slot(a)));
            }
            _ => {
                // Default sort by slot descending
                all_events.sort_by(|a, b| self.get_event_slot(b).cmp(&self.get_event_slot(a)));
            }
        }
        
        let total = all_events.len();
        let offset = (page - 1) * limit;
        let has_prev = page > 1;
        let has_next = offset + limit < total;
        
        // Pagination
        let events = all_events
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        
        Ok(EventQueryResponse {
            events,
            total,
            page,
            limit,
            has_next,
            has_prev,
        })
    }

    /// Get event slot
    fn get_event_slot(&self, event: &SpinPetEvent) -> u64 {
        match event {
            SpinPetEvent::TokenCreated(e) => e.slot,
            SpinPetEvent::BuySell(e) => e.slot,
            SpinPetEvent::LongShort(e) => e.slot,
            SpinPetEvent::ForceLiquidate(e) => e.slot,
            SpinPetEvent::FullClose(e) => e.slot,
            SpinPetEvent::PartialClose(e) => e.slot,
        }
    }

    /// Query all mint information
    pub async fn query_mints(&self, query: MintQuery) -> Result<MintQueryResponse> {
        let page = query.page.unwrap_or(1);
        let limit = query.limit.unwrap_or(50);
        
        debug!("🔍 Querying mint information, page: {}, limit: {}", page, limit);
        
        // Collect all mint information
        let mut all_mints = Vec::new();
        
        let prefix = "mt:";
        let iter = self.db.iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));
        
        for item in iter {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            
            // Check if still matches prefix
            if !key_str.starts_with(prefix) {
                break;
            }
            
            // Parse key format: mt:{mint_account}
            let parts: Vec<&str> = key_str.split(':').collect();
            if parts.len() >= 2 {
                let mint_account = parts[1];
                
                all_mints.push(mint_account.to_string());
            }
        }
        
        // Sort by mint_account (alphabetically)
        all_mints.sort_by(|a, b| a.cmp(&b));
        
        let total = all_mints.len();
        let offset = (page - 1) * limit;
        let has_prev = page > 1;
        let has_next = offset + limit < total;
        
        // Pagination
        let mints = all_mints
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        
        Ok(MintQueryResponse {
            mints,
            total,
            page,
            limit,
            has_next,
            has_prev,
        })
    }

    /// Query order information
    pub async fn query_orders(&self, query: OrderQuery) -> Result<OrderQueryResponse> {
        let mint_account = &query.mint_account;
        let order_type = &query.order_type;
        
        debug!("🔍 Querying order information, mint: {}, type: {}", mint_account, order_type);
        
        // Determine search prefix
        let type_str = match order_type.as_str() {
            "up_orders" => "up",
            "down_orders" => "dn",
            _ => return Err(anyhow::anyhow!("Invalid order type: {}", order_type)),
        };
        
        let prefix = format!("or:{}:{}:", mint_account, type_str);
        let mut orders = Vec::new();
        
        let iter = self.db.iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));
        
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            
            // Check if still matches prefix
            if !key_str.starts_with(&prefix) {
                break;
            }
            
            // Parse order data
            match serde_json::from_slice::<OrderData>(&value) {
                Ok(order_data) => orders.push(order_data),
                Err(e) => {
                    error!("❌ Failed to parse order data: {}, key: {}", e, key_str);
                    continue;
                }
            }
        }
        
        let total = orders.len();
        
        Ok(OrderQueryResponse {
            orders,
            total,
            order_type: order_type.clone(),
            mint_account: mint_account.clone(),
        })
    }

    /// Query user transaction information
    pub async fn query_user_transactions(&self, query: UserQuery) -> Result<UserQueryResponse> {
        let user = &query.user;
        let mint_account = &query.mint_account;
        let page = query.page.unwrap_or(1);
        let limit = query.limit.unwrap_or(50);
        let order_by = query.order_by.unwrap_or_else(|| "slot_desc".to_string());
        
        debug!("🔍 Querying user transaction information, user: {}, mint: {:?}, page: {}, limit: {}, order: {}", 
               user, mint_account, page, limit, order_by);
        
        // Build search prefix
        let prefix = if let Some(mint) = mint_account {
            format!("us:{}:{}:", user, mint)
        } else {
            format!("us:{}:", user)
        };
        
        let mut all_transactions = Vec::new();
        let iter = self.db.iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));
        
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            
            // Check if still matches prefix
            if !key_str.starts_with(&prefix) {
                break;
            }
            
            // Parse user transaction data
            match serde_json::from_slice::<UserTransactionData>(&value) {
                Ok(transaction_data) => {
                    all_transactions.push(transaction_data);
                }
                Err(e) => {
                    error!("❌ Failed to parse user transaction data: {}, key: {}", e, key_str);
                    continue;
                }
            }
        }
        
        // Sort by slot
        match order_by.as_str() {
            "slot_asc" => {
                all_transactions.sort_by(|a, b| a.slot.cmp(&b.slot));
            }
            "slot_desc" => {
                all_transactions.sort_by(|a, b| b.slot.cmp(&a.slot));
            }
            _ => {
                // Default sort by slot descending
                all_transactions.sort_by(|a, b| b.slot.cmp(&a.slot));
            }
        }
        
        let total = all_transactions.len();
        let offset = (page - 1) * limit;
        let has_prev = page > 1;
        let has_next = offset + limit < total;
        
        // Pagination
        let transactions = all_transactions
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        
        Ok(UserQueryResponse {
            transactions,
            total,
            page,
            limit,
            has_next,
            has_prev,
            user: user.clone(),
            mint_account: mint_account.clone(),
        })
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<String> {
        let stats = self.db.property_value("rocksdb.stats")?;
        Ok(stats.unwrap_or_else(|| "No stats available".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use chrono::Utc;

    #[tokio::test]
    async fn test_event_storage() {
        let temp_dir = TempDir::new().unwrap();
        let config = DatabaseConfig {
            rocksdb_path: temp_dir.path().to_str().unwrap().to_string(),
        };
        
        let storage = EventStorage::new(&config).unwrap();
        
        // Create test event
        let event = SpinPetEvent::TokenCreated(TokenCreatedEvent {
            payer: "test_payer".to_string(),
            mint_account: "test_mint".to_string(),
            curve_account: "test_curve".to_string(),
            name: "Test Token".to_string(),
            symbol: "TT".to_string(),
            uri: "https://test.com".to_string(),
            timestamp: Utc::now(),
            signature: "test_signature".to_string(),
            slot: 12345,
        });
        
        // Store event
        storage.store_event(event).await.unwrap();
        
        // Query events
        let query = EventQuery {
            mint_account: "test_mint".to_string(),
            page: Some(1),
            limit: Some(10),
            order_by: Some("slot_desc".to_string()),
        };
        
        let response = storage.query_events(query).await.unwrap();
        assert_eq!(response.events.len(), 1);
        assert_eq!(response.total, 1);
    }
} 
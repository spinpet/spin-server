use std::sync::Arc;
use std::time::Duration;
use rocksdb::{DB, Options, IteratorMode, Direction};
use tracing::{info, error, debug, warn};
use serde::{Serialize, Deserialize};
use anyhow::Result;
use serde_with::{serde_as, DisplayFromStr};
use chrono::{DateTime, Utc};
use tokio::time::sleep;

use crate::solana::events::*;
use crate::config::{DatabaseConfig, Config};
use crate::models::{KlineData, KlineQuery, KlineQueryResponse};

/// Event type constants - used for key generation (2 characters to save space)
pub const EVENT_TYPE_TOKEN_CREATED: &str = "tc";
pub const EVENT_TYPE_BUY_SELL: &str = "bs";
pub const EVENT_TYPE_LONG_SHORT: &str = "ls";
pub const EVENT_TYPE_FORCE_LIQUIDATE: &str = "fl";
pub const EVENT_TYPE_FULL_CLOSE: &str = "fc";
pub const EVENT_TYPE_PARTIAL_CLOSE: &str = "pc";
pub const EVENT_TYPE_MILESTONE_DISCOUNT: &str = "md";

/// Kline interval constants - used for key generation (2-3 characters to save space)
pub const KLINE_INTERVAL_1S: &str = "s1";
pub const KLINE_INTERVAL_30S: &str = "s30";
pub const KLINE_INTERVAL_5M: &str = "m5";

/// Precision constant for u128 to f64 conversion (28 decimal places)
pub const PRICE_PRECISION: u128 = 10_u128.pow(28);

/// Event storage service
pub struct EventStorage {
    db: Arc<DB>,
    config: Config,
    http_client: reqwest::Client,
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
    pub sort_by: Option<String>,  // "slot_asc", "slot_desc"
    pub cursor: Option<String>,   // 用于高效分页的游标
}

/// Mint information
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MintInfo {
    pub mint_account: String,
    pub slot: u64,
    pub created_at: Option<i64>,  // timestamp derived from slot
}

/// Mint query response
#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct MintQueryResponse {
    pub mints: Vec<String>,       // 直接返回mint地址字符串数组，减少数据传输量
    pub total: Option<usize>,     // 对于大数据集，计算总数可能很慢，设为可选
    pub page: usize,
    pub limit: usize,
    pub has_next: bool,
    pub has_prev: bool,
    pub next_cursor: Option<String>,  // 下一页的游标
    pub sort_by: String,
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
    pub page: Option<usize>,
    pub limit: Option<usize>,
}

/// Order query response
#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct OrderQueryResponse {
    pub orders: Vec<OrderData>,
    pub total: usize,
    pub order_type: String,
    pub mint_account: String,
    pub page: usize,
    pub limit: usize,
    pub has_next: bool,
    pub has_prev: bool,
}

/// User order query parameters
#[derive(Debug, Serialize, Deserialize)]
pub struct UserOrderQuery {
    pub user: String,
    pub mint_account: Option<String>, // Optional mint account for more precise query
    pub page: Option<usize>,
    pub limit: Option<usize>,
    pub order_by: Option<String>, // "start_time_asc" or "start_time_desc"
}

/// User order query response
#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct UserOrderQueryResponse {
    pub orders: Vec<OrderData>,
    pub total: usize,
    pub user: String,
    pub mint_account: Option<String>, // The mint account used in query (if specified)
    pub page: usize,
    pub limit: usize,
    pub has_next: bool,
    pub has_prev: bool,
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

/// Token URI metadata information from IPFS
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema, Default, Clone)]
pub struct TokenUriData {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    #[serde(rename = "showName")]
    pub show_name: Option<bool>,
    #[serde(rename = "createdOn")]
    pub created_on: Option<String>,
    pub twitter: Option<String>,
    pub website: Option<String>,
    pub telegram: Option<String>,
}

/// Mint detail information
#[serde_as]
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema, Default)]
pub struct MintDetailData {
    pub mint_account: String,
    pub payer: Option<String>,
    pub curve_account: Option<String>,
    pub pool_token_account: Option<String>,
    pub pool_sol_account: Option<String>,
    pub fee_recipient: Option<String>,
    pub base_fee_recipient: Option<String>,        // 基础手续费接收账户
    pub params_account: Option<String>,            // 合作伙伴参数账户PDA地址
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub uri: Option<String>,
    pub swap_fee: Option<u16>,                     // 现货交易手续费
    pub borrow_fee: Option<u16>,                   // 保证金交易手续费
    pub fee_discount_flag: Option<u8>,             // 手续费折扣标志 0: 原价 1: 5折 2: 2.5折  3: 1.25折
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
    pub created_by: Option<String>,
    #[schema(value_type = Option<String>)]
    pub last_updated_at: Option<DateTime<Utc>>,
    pub uri_data: Option<TokenUriData>,
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
    pub fn new(config: &Config) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // 1. Maximize memory usage - reduce flush frequency
        opts.set_write_buffer_size(512 * 1024 * 1024);     // 512MB single buffer
        opts.set_max_write_buffer_number(8);               // 8 buffers = 4GB memory
        opts.set_min_write_buffer_number_to_merge(1);      // Single buffer can flush
        opts.set_db_write_buffer_size(4096 * 1024 * 1024); // 4GB total write buffer
        
        // 2. Progressive compression (balance performance and space)
        opts.set_compression_type(rocksdb::DBCompressionType::None);
        opts.set_compression_per_level(&[
            rocksdb::DBCompressionType::None,        // L0: No compression (latest data, frequent writes)
            rocksdb::DBCompressionType::None,        // L1: No compression (frequent writes)
            rocksdb::DBCompressionType::Snappy,      // L2: Light compression
            rocksdb::DBCompressionType::Lz4,         // L3: Light compression
            rocksdb::DBCompressionType::Zstd,        // L4: Medium compression
            rocksdb::DBCompressionType::Zstd,        // L5: Medium compression
            rocksdb::DBCompressionType::Zstd,        // L6: Medium compression
        ]);
        
        // 3. Greatly delay compaction triggers - almost no compaction
        opts.set_level_zero_file_num_compaction_trigger(50);  // 50 L0 files before compaction
        opts.set_level_zero_slowdown_writes_trigger(100);     // 100 files before slowdown
        opts.set_level_zero_stop_writes_trigger(200);         // 200 files before stop
        
        // 4. Ultra-large file sizes - reduce file count
        opts.set_target_file_size_base(1024 * 1024 * 1024);   // 1GB file size
        opts.set_max_bytes_for_level_base(10 * 1024 * 1024 * 1024); // 10GB L1 size
        opts.set_max_bytes_for_level_multiplier(10.0);        // 10x growth per level
        opts.set_num_levels(7);
        
        // 5. Maximize concurrency
        opts.set_max_background_jobs(16);                     // 16 background tasks
        opts.set_max_subcompactions(8);                       // 8 sub-compaction tasks
        
        // 6. Ultimate filesystem optimization
        opts.set_use_fsync(false);                            // Disable fsync
        opts.set_bytes_per_sync(0);                           // Disable periodic sync
        opts.set_wal_bytes_per_sync(0);                       // Disable WAL sync
        
        // 7. WAL ultimate optimization
        opts.set_max_total_wal_size(2048 * 1024 * 1024);     // 2GB WAL
        
        // 8. Disable all statistics and checks
        opts.set_stats_dump_period_sec(0);                   // Disable stats
        opts.set_stats_persist_period_sec(0);                // Disable stats persistence
        opts.set_paranoid_checks(false);                     // Disable paranoid checks
        
        // 9. Memory table optimization
        opts.set_allow_concurrent_memtable_write(true);      // Concurrent memtable writes
        opts.set_enable_write_thread_adaptive_yield(true);   // Adaptive yield
        opts.set_max_open_files(-1);                         // Unlimited open files
        
        // 10. Optimize memory allocation
        opts.set_arena_block_size(64 * 1024 * 1024);         // 64MB arena blocks
        
        let db = DB::open(&opts, &config.database.rocksdb_path)?;
        
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.ipfs.request_timeout_seconds))
            .build()?;
        
        info!("🗄️ RocksDB initialized successfully, path: {}", config.database.rocksdb_path);
        Ok(Self {
            db: Arc::new(db),
            config: config.clone(),
            http_client,
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
            SpinPetEvent::MilestoneDiscount(e) => (
                &e.mint_account,
                e.slot,
                &e.signature,
                EVENT_TYPE_MILESTONE_DISCOUNT
            ),
        };
        
        // Format slot as 10 digits with leading zeros, for correct sorting by dictionary order
        format!("tr:{}:{:010}:{}:{}", mint_account, slot, event_type, signature)
    }

    /// Generate mint marker key (slot-based index)
    /// Format: mt:{slot:010}:{mint_account}
    fn generate_mint_key(&self, slot: u64, mint_account: &str) -> String {
        format!("mt:{:010}:{}", slot, mint_account)
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

    /// Generate user order key
    /// Format: uo:{user}:{mint}:{order_pda}
    fn generate_user_order_key(&self, user: &str, mint: &str, order_pda: &str) -> String {
        format!("uo:{}:{}:{}", user, mint, order_pda)
    }

    /// Generate kline key
    /// Format: {interval}:{mint_account}:{timestamp_padded}
    fn generate_kline_key(&self, interval: &str, mint_account: &str, timestamp: u64) -> String {
        format!("{}:{}:{:020}", interval, mint_account, timestamp)
    }

    /// Convert u128 price to f64 with 28-bit precision handling
    fn convert_price_to_f64(&self, price_u128: u128) -> f64 {
        // Convert u128 to f64 with precision handling
        // Since u128 has 28 decimal places, we divide by 10^28
        // But f64 has limited precision, so we might lose some accuracy
        let price_f64 = price_u128 as f64 / PRICE_PRECISION as f64;
        
        // Round to reasonable precision (e.g., 12 decimal places) to avoid floating point noise
        (price_f64 * 1e12).round() / 1e12
    }

    /// Calculate time bucket for different intervals
    /// Returns the aligned timestamp for the time bucket
    fn calculate_time_bucket(&self, timestamp: u64, interval: &str) -> u64 {
        match interval {
            KLINE_INTERVAL_1S => timestamp, // 1-second intervals - no alignment needed
            KLINE_INTERVAL_30S => {
                // 30-second intervals - align to 30-second boundary
                // Floor timestamp to 30-second boundary, then return the aligned timestamp
                (timestamp / 30) * 30
            },
            KLINE_INTERVAL_5M => {
                // 5-minute intervals - align to 5-minute boundary
                // Floor timestamp to 5-minute boundary, then return the aligned timestamp
                (timestamp / 300) * 300
            },
            _ => timestamp, // default to 1-second
        }
    }

    /// Get order by PDA for user order operations
    async fn get_order_by_pda(&self, mint_account: &str, order_type: u8, order_pda: &str) -> Result<Option<OrderData>> {
        let order_key = self.generate_order_key(mint_account, order_type, order_pda);
        match self.db.get(order_key.as_bytes())? {
            Some(data) => {
                match serde_json::from_slice::<OrderData>(&data) {
                    Ok(order_data) => Ok(Some(order_data)),
                    Err(e) => {
                        error!("❌ Failed to parse order data: {}, key: {}", e, order_key);
                        Ok(None)
                    }
                }
            }
            None => Ok(None),
        }
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
            SpinPetEvent::MilestoneDiscount(e) => {
                Some(UserTransactionData {
                    event_type: "milestone_discount".to_string(),
                    user: e.payer.clone(), // Use payer as user
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

    /// Process kline data for price events
    async fn process_kline_data(&self, mint_account: &str, latest_price: u128, timestamp: DateTime<Utc>) -> Result<()> {
        let price = self.convert_price_to_f64(latest_price);
        let unix_timestamp = timestamp.timestamp() as u64;
        
        let intervals = [KLINE_INTERVAL_1S, KLINE_INTERVAL_30S, KLINE_INTERVAL_5M];
        
        for interval in intervals {
            let time_bucket = self.calculate_time_bucket(unix_timestamp, interval);
            let kline_key = self.generate_kline_key(interval, mint_account, time_bucket);
            
            // Try to get existing kline data
            let kline_data = match self.db.get(kline_key.as_bytes())? {
                Some(data) => {
                    match serde_json::from_slice::<KlineData>(&data) {
                        Ok(mut existing_kline) => {
                            // Update existing kline data
                            existing_kline.high = existing_kline.high.max(price);
                            existing_kline.low = existing_kline.low.min(price);
                            existing_kline.close = price;
                            existing_kline.update_count += 1;
                            existing_kline.is_final = false; // Mark as not final since it's being updated
                            existing_kline
                        }
                        Err(e) => {
                            warn!("Failed to parse existing kline data: {}, creating new one", e);
                            // Create new kline data if parsing fails
                            KlineData {
                                time: time_bucket,
                                open: price,
                                high: price,
                                low: price,
                                close: price,
                                volume: 0.0, // Volume is 0 as requested
                                is_final: false,
                                update_count: 1,
                            }
                        }
                    }
                }
                None => {
                    // Create new kline data
                    KlineData {
                        time: time_bucket,
                        open: price,
                        high: price,
                        low: price,
                        close: price,
                        volume: 0.0, // Volume is 0 as requested
                        is_final: false,
                        update_count: 1,
                    }
                }
            };
            
            // Store updated kline data
            let value = serde_json::to_vec(&kline_data)?;
            self.db.put(kline_key.as_bytes(), &value)?;
            
            debug!("💹 Kline data updated for interval {}, mint: {}, time: {}, price: {}", 
                   interval, mint_account, time_bucket, price);
        }
        
        Ok(())
    }

    /// Generate mint detail key
    /// Format: in:{mint_account}
    fn generate_mint_detail_key(&self, mint_account: &str) -> String {
        format!("in:{}", mint_account)
    }

    /// Extract IPFS hash from URI
    fn extract_ipfs_hash(uri: &str) -> Option<String> {
        if let Some(hash) = uri.strip_prefix("https://ipfs.io/ipfs/") {
            Some(hash.to_string())
        } else if uri.starts_with("ipfs://") {
            Some(uri[7..].to_string())
        } else {
            // Try to extract hash from other common IPFS patterns
            if uri.contains("/ipfs/") {
                if let Some(pos) = uri.find("/ipfs/") {
                    let start = pos + 6; // "/ipfs/".len()
                    let hash = &uri[start..];
                    // Find the end of the hash (before any query params or fragments)
                    let end_pos = hash.find('?').or_else(|| hash.find('#')).unwrap_or(hash.len());
                    Some(hash[..end_pos].to_string())
                } else {
                    None
                }
            } else {
                None
            }
        }
    }

    /// Fetch token metadata from IPFS with retry logic
    async fn fetch_token_uri_data(&self, uri: &str) -> Option<TokenUriData> {
        let ipfs_hash = Self::extract_ipfs_hash(uri)?;
        let ipfs_url = format!("{}{}", self.config.ipfs.gateway_url, ipfs_hash);
        
        debug!("Fetching token metadata from: {}", ipfs_url);
        
        for attempt in 1..=self.config.ipfs.max_retries {
            match self.http_client.get(&ipfs_url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        match response.json::<TokenUriData>().await {
                            Ok(uri_data) => {
                                debug!("Successfully fetched token metadata for URI: {}", uri);
                                return Some(uri_data);
                            }
                            Err(e) => {
                                warn!("Failed to parse JSON from IPFS (attempt {}/{}): {}", attempt, self.config.ipfs.max_retries, e);
                            }
                        }
                    } else {
                        warn!("HTTP error from IPFS gateway (attempt {}/{}): {}", attempt, self.config.ipfs.max_retries, response.status());
                    }
                }
                Err(e) => {
                    warn!("Network error fetching from IPFS (attempt {}/{}): {}", attempt, self.config.ipfs.max_retries, e);
                }
            }
            
            // Sleep before retry (except on last attempt)
            if attempt < self.config.ipfs.max_retries {
                sleep(Duration::from_secs(self.config.ipfs.retry_delay_seconds)).await;
            }
        }
        
        error!("Failed to fetch token metadata after {} attempts for URI: {}", self.config.ipfs.max_retries, uri);
        None
    }

    /// Update mint detail with URI data
    async fn update_mint_uri_data(&self, mint_account: &str, uri_data: TokenUriData) -> Result<()> {
        let key = self.generate_mint_detail_key(mint_account);
        
        // Get existing detail
        let mut detail = match self.db.get(key.as_bytes())? {
            Some(data) => serde_json::from_slice::<MintDetailData>(&data)
                .unwrap_or_else(|_| MintDetailData {
                    mint_account: mint_account.to_string(),
                    ..Default::default()
                }),
            None => {
                warn!("Mint detail not found for URI update: {}", mint_account);
                return Ok(());
            }
        };
        
        // Update URI data
        detail.uri_data = Some(uri_data);
        detail.last_updated_at = Some(Utc::now());
        
        // Save back to database
        let value = serde_json::to_vec(&detail)?;
        self.db.put(key.as_bytes(), &value)?;
        
        debug!("✅ URI data updated successfully for mint: {}", mint_account);
        Ok(())
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
            SpinPetEvent::MilestoneDiscount(e) => &e.mint_account,
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
        
        // Update detail based on event type
        match event {
            SpinPetEvent::TokenCreated(e) => {
                detail.payer = Some(e.payer.clone());
                detail.curve_account = Some(e.curve_account.clone());
                detail.pool_token_account = Some(e.pool_token_account.clone());
                detail.pool_sol_account = Some(e.pool_sol_account.clone());
                detail.fee_recipient = Some(e.fee_recipient.clone());
                detail.base_fee_recipient = Some(e.base_fee_recipient.clone());
                detail.params_account = Some(e.params_account.clone());
                detail.swap_fee = Some(e.swap_fee);
                detail.borrow_fee = Some(e.borrow_fee);
                detail.fee_discount_flag = Some(e.fee_discount_flag);
                detail.name = Some(e.name.clone());
                detail.symbol = Some(e.symbol.clone());
                detail.uri = Some(e.uri.clone());
                detail.create_timestamp = Some(e.timestamp.timestamp());
                detail.created_by = Some(e.payer.clone());
                detail.last_updated_at = Some(e.timestamp);
            },
            SpinPetEvent::MilestoneDiscount(e) => {
                // Update fee-related fields from MilestoneDiscount event
                detail.swap_fee = Some(e.swap_fee);
                detail.borrow_fee = Some(e.borrow_fee);
                detail.fee_discount_flag = Some(e.fee_discount_flag);
                detail.last_updated_at = Some(e.timestamp);
            },
            SpinPetEvent::BuySell(e) => { 
                detail.latest_price = Some(e.latest_price);
                detail.latest_trade_time = Some(e.timestamp.timestamp());
                detail.total_sol_amount = detail.total_sol_amount.saturating_add(e.sol_amount);
                detail.last_updated_at = Some(e.timestamp);
            },
            SpinPetEvent::LongShort(e) => {
                detail.latest_price = Some(e.latest_price);
                detail.latest_trade_time = Some(e.timestamp.timestamp());
                detail.total_margin_sol_amount = detail.total_margin_sol_amount.saturating_add(e.margin_sol_amount);
                detail.last_updated_at = Some(e.timestamp);
            },
            SpinPetEvent::ForceLiquidate(e) => {
                detail.total_force_liquidations = detail.total_force_liquidations.saturating_add(1);
                detail.last_updated_at = Some(e.timestamp);
            },
            SpinPetEvent::FullClose(e) => {
                detail.latest_price = Some(e.latest_price);
                detail.latest_trade_time = Some(e.timestamp.timestamp());
                detail.total_close_profit = detail.total_close_profit.saturating_add(e.user_close_profit);
                detail.last_updated_at = Some(e.timestamp);
            },
            SpinPetEvent::PartialClose(e) => {
                detail.latest_price = Some(e.latest_price);
                detail.latest_trade_time = Some(e.timestamp.timestamp());
                detail.total_close_profit = detail.total_close_profit.saturating_add(e.user_close_profit);
                detail.last_updated_at = Some(e.timestamp);
            },
        }
        
        let value = serde_json::to_vec(&detail)?;
        self.db.put(key.as_bytes(), &value)?;
        
        debug!("💾 Mint detail updated successfully, key: {}", key);
        
        // For TokenCreated events, fetch URI data asynchronously if URI is present
        if let SpinPetEvent::TokenCreated(token_event) = event {
            if !token_event.uri.is_empty() {
                let storage = Self {
                    db: self.db.clone(),
                    config: self.config.clone(),
                    http_client: self.http_client.clone(),
                };
                let uri = token_event.uri.clone();
                let mint_account = token_event.mint_account.clone();
                
                // Spawn async task to fetch URI data without blocking
                tokio::spawn(async move {
                    if let Some(uri_data) = storage.fetch_token_uri_data(&uri).await {
                        if let Err(e) = storage.update_mint_uri_data(&mint_account, uri_data).await {
                            error!("Failed to update URI data for mint {}: {}", mint_account, e);
                        }
                    }
                });
            }
        }
        
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
        
        let mut batch = rocksdb::WriteBatch::default();
        batch.put(key.as_bytes(), &value);
        
        // Only store mint marker for TokenCreatedEvent and avoid duplicates
        if let SpinPetEvent::TokenCreated(token_event) = &event {
            let mint_detail_key = self.generate_mint_detail_key(&token_event.mint_account);
            
            // Check if mint already exists using in: key to avoid duplicates
            if self.db.get(mint_detail_key.as_bytes())?.is_none() {
                let mint_key = self.generate_mint_key(token_event.slot, &token_event.mint_account);
                batch.put(mint_key.as_bytes(), b""); // Empty value marker
                debug!("💾 New mint marker stored: {}", mint_key);
            } else {
                debug!("⚠️ Mint already exists (found in: key), skipping mint marker for: {}", token_event.mint_account);
            }
        }
        
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
                
                // Create user order data
                let user_order_key = self.generate_user_order_key(&long_short_event.user, &long_short_event.mint_account, &long_short_event.order_pda);
                batch.put(user_order_key.as_bytes(), &order_value);
                debug!("💾 User order data stored successfully, key: {}", user_order_key);
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
                
                // Update user order data
                let user_order_key = self.generate_user_order_key(&partial_close_event.user, &partial_close_event.mint_account, &partial_close_event.order_pda);
                batch.put(user_order_key.as_bytes(), &order_value);
                debug!("💾 User order data updated successfully, key: {}", user_order_key);
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
                
                // Delete user order data - need to find user from existing order
                if let Some(existing_order) = self.get_order_by_pda(&full_close_event.mint_account, order_type, &full_close_event.order_pda).await? {
                    let user_order_key = self.generate_user_order_key(&existing_order.user, &full_close_event.mint_account, &full_close_event.order_pda);
                    batch.delete(user_order_key.as_bytes());
                    debug!("💾 User order data deleted successfully, key: {}", user_order_key);
                }
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
                    
                    // Delete user order data for up order
                    if let Some(existing_order) = self.get_order_by_pda(&force_liquidate_event.mint_account, 2, &force_liquidate_event.order_pda).await? {
                        let user_order_key = self.generate_user_order_key(&existing_order.user, &force_liquidate_event.mint_account, &force_liquidate_event.order_pda);
                        batch.delete(user_order_key.as_bytes());
                        debug!("💾 User order data deleted successfully for up order, key: {}", user_order_key);
                    }
                }
                if self.db.get(dn_key.as_bytes())?.is_some() {
                    batch.delete(dn_key.as_bytes());
                    debug!("💾 Force liquidation order deleted successfully, key: {}", dn_key);
                    
                    // Delete user order data for dn order
                    if let Some(existing_order) = self.get_order_by_pda(&force_liquidate_event.mint_account, 1, &force_liquidate_event.order_pda).await? {
                        let user_order_key = self.generate_user_order_key(&existing_order.user, &force_liquidate_event.mint_account, &force_liquidate_event.order_pda);
                        batch.delete(user_order_key.as_bytes());
                        debug!("💾 User order data deleted successfully for dn order, key: {}", user_order_key);
                    }
                }
            }
            SpinPetEvent::MilestoneDiscount(milestone_discount_event) => {
                // MilestoneDiscountEvent doesn't have a user field, we need to get user info from order_pda
                // This requires additional query, for now we'll use payer as user
                let user_transaction = UserTransactionData {
                    event_type: "milestone_discount".to_string(),
                    user: milestone_discount_event.payer.clone(), // Use payer as user
                    mint_account: milestone_discount_event.mint_account.clone(),
                    slot: milestone_discount_event.slot,
                    timestamp: milestone_discount_event.timestamp.timestamp(),
                    signature: milestone_discount_event.signature.clone(),
                    event_data: serde_json::to_value(milestone_discount_event).unwrap_or(serde_json::Value::Null),
                };
                let user_key = self.generate_user_transaction_key(
                    &user_transaction.user,
                    &user_transaction.mint_account,
                    user_transaction.slot
                );
                let user_value = serde_json::to_vec(&user_transaction)?;
                batch.put(user_key.as_bytes(), &user_value);
                debug!("💾 User transaction recorded successfully, key: {}", user_key);
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

         // Process kline data for price events
         match &event {
             SpinPetEvent::BuySell(e) => {
                 if let Err(err) = self.process_kline_data(&e.mint_account, e.latest_price, e.timestamp).await {
                     error!("❌ Failed to process kline data for BuySell event: {}", err);
                 }
             }
             SpinPetEvent::LongShort(e) => {
                 if let Err(err) = self.process_kline_data(&e.mint_account, e.latest_price, e.timestamp).await {
                     error!("❌ Failed to process kline data for LongShort event: {}", err);
                 }
             }
             SpinPetEvent::FullClose(e) => {
                 if let Err(err) = self.process_kline_data(&e.mint_account, e.latest_price, e.timestamp).await {
                     error!("❌ Failed to process kline data for FullClose event: {}", err);
                 }
             }
             SpinPetEvent::PartialClose(e) => {
                 if let Err(err) = self.process_kline_data(&e.mint_account, e.latest_price, e.timestamp).await {
                     error!("❌ Failed to process kline data for PartialClose event: {}", err);
                 }
             }
             _ => {
                 // Other events don't have latest_price, so no kline processing needed
             }
         }

         // Process mint detail data
         self.process_event_for_mint_detail(&event).await?;
         
         self.db.write(batch)?;
        
        debug!("💾 Event stored successfully, key: {}", key);
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn store_events(&self, events: Vec<SpinPetEvent>) -> Result<()> {
        let mut batch = rocksdb::WriteBatch::default();
        let mut processed_mints = std::collections::HashSet::new();
        
        for event in &events {
            let key = self.generate_event_key(event);
            let value = serde_json::to_vec(event)?;
            batch.put(key.as_bytes(), &value);
            
            // Only store mint marker for TokenCreatedEvent and avoid duplicates
            if let SpinPetEvent::TokenCreated(token_event) = event {
                // Check if already processed in this batch
                if !processed_mints.contains(&token_event.mint_account) {
                    let mint_detail_key = self.generate_mint_detail_key(&token_event.mint_account);
                    
                    // Check if mint already exists using in: key to avoid duplicates
                    if self.db.get(mint_detail_key.as_bytes())?.is_none() {
                        let mint_key = self.generate_mint_key(token_event.slot, &token_event.mint_account);
                        batch.put(mint_key.as_bytes(), b""); // Empty value marker
                        processed_mints.insert(token_event.mint_account.clone());
                        debug!("💾 New mint marker stored in batch: {}", mint_key);
                    } else {
                        debug!("⚠️ Mint already exists in DB (found in: key), skipping: {}", token_event.mint_account);
                    }
                }
            }
            
            // Process order-related events for user order data
            match event {
                SpinPetEvent::LongShort(long_short_event) => {
                    let order_data = self.create_order_data_from_long_short(long_short_event);
                    let user_order_key = self.generate_user_order_key(&long_short_event.user, &long_short_event.mint_account, &long_short_event.order_pda);
                    let order_value = serde_json::to_vec(&order_data)?;
                    batch.put(user_order_key.as_bytes(), &order_value);
                    debug!("💾 User order data stored in batch: {}", user_order_key);
                }
                SpinPetEvent::PartialClose(partial_close_event) => {
                    let order_data = self.create_order_data_from_partial_close(partial_close_event);
                    let user_order_key = self.generate_user_order_key(&partial_close_event.user, &partial_close_event.mint_account, &partial_close_event.order_pda);
                    let order_value = serde_json::to_vec(&order_data)?;
                    batch.put(user_order_key.as_bytes(), &order_value);
                    debug!("💾 User order data updated in batch: {}", user_order_key);
                }
                _ => {}
            }
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
            SpinPetEvent::MilestoneDiscount(e) => e.slot,
        }
    }

    /// Query all mint information with efficient slot-based sorting and pagination
    pub async fn query_mints(&self, query: MintQuery) -> Result<MintQueryResponse> {
        let limit = query.limit.unwrap_or(50).min(1000); // 限制最大1000条
        let sort_by = query.sort_by.unwrap_or_else(|| "slot_desc".to_string());
        
        debug!("🔍 Querying mint information, limit: {}, sort_by: {}", limit, sort_by);
        
        let prefix = "mt:";
        let mut mints = Vec::new();
        let mut next_cursor = None;
        
        // 根据排序方向选择迭代器方向
        let (iterator, direction_desc) = match sort_by.as_str() {
            "slot_asc" => {
                // 升序：从最小开始迭代
                let start_key = query.cursor.as_deref().unwrap_or(prefix);
                (self.db.iterator(IteratorMode::From(start_key.as_bytes(), Direction::Forward)), false)
            }
            "slot_desc" => {
                // 降序：从最大开始反向迭代
                if let Some(cursor) = &query.cursor {
                    (self.db.iterator(IteratorMode::From(cursor.as_bytes(), Direction::Reverse)), true)
                } else {
                    // 从最大的mt:键开始（mt:zzzzzzzzzz）
                    let start_key = "mt:~"; // ASCII中~比所有数字字母都大
                    (self.db.iterator(IteratorMode::From(start_key.as_bytes(), Direction::Reverse)), true)
                }
            }
            _ => {
                return Err(anyhow::anyhow!("Invalid sort_by parameter: {}, must be 'slot_asc' or 'slot_desc'", sort_by));
            }
        };
        
        let mut count = 0;
        let mut skip_first = query.cursor.is_some(); // 如果有cursor，跳过第一个（它是上一页的最后一个）
        
        for item in iterator {
            let (key, _) = item?;
            let key_str = String::from_utf8_lossy(&key);
            
            // 检查是否仍然匹配前缀
            if !key_str.starts_with(prefix) {
                if direction_desc {
                    // 反向迭代时，如果不匹配前缀说明已经超出范围
                    break;
                } else {
                    // 正向迭代时，如果不匹配前缀说明已经超出范围
                    break;
                }
            }
            
            // 如果有cursor且是第一条记录，跳过（避免重复）
            if skip_first {
                skip_first = false;
                continue;
            }
            
            // 解析键格式: mt:{slot:010}:{mint_account}
            let parts: Vec<&str> = key_str.splitn(3, ':').collect();
            if parts.len() >= 3 {
                let slot_str = parts[1];
                let mint_account = parts[2];
                
                if let Ok(_slot) = slot_str.parse::<u64>() {
                    mints.push(mint_account.to_string());
                    
                    count += 1;
                    
                    // 达到限制数量，设置下一页游标
                    if count >= limit {
                        next_cursor = Some(key_str.to_string());
                        break;
                    }
                }
            }
        }
        
        let has_next = next_cursor.is_some();
        let has_prev = query.cursor.is_some(); // 如果有cursor说明不是第一页
        
        debug!("🔍 Retrieved {} mints, has_next: {}, has_prev: {}", mints.len(), has_next, has_prev);
        
        Ok(MintQueryResponse {
            mints,
            total: None, // 对于大数据集，不计算总数以保持性能
            page: query.page.unwrap_or(1),
            limit,
            has_next,
            has_prev,
            next_cursor,
            sort_by,
        })
    }

    /// Query order information
    pub async fn query_orders(&self, query: OrderQuery) -> Result<OrderQueryResponse> {
        let mint_account = &query.mint_account;
        let order_type = &query.order_type;
        let page = query.page.unwrap_or(1);
        let limit = query.limit.unwrap_or(50);
        
        debug!("🔍 Querying order information, mint: {}, type: {}, page: {}, limit: {}", mint_account, order_type, page, limit);
        
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
        
        // Sort orders based on lock_lp_start_price
        match order_type.as_str() {
            "up_orders" => {
                // For up_orders: sort by lock_lp_start_price ascending (small to large)
                orders.sort_by(|a, b| a.lock_lp_start_price.cmp(&b.lock_lp_start_price));
            },
            "down_orders" => {
                // For down_orders: sort by lock_lp_start_price descending (large to small)
                orders.sort_by(|a, b| b.lock_lp_start_price.cmp(&a.lock_lp_start_price));
            },
            _ => {} // Should never reach here due to check above
        }
        
        let total = orders.len();
        
        let offset = (page - 1) * limit;
        let has_prev = page > 1;
        let has_next = offset + limit < total;
        
        // Apply pagination
        let orders = orders
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        
        Ok(OrderQueryResponse {
            orders,
            total,
            order_type: order_type.clone(),
            mint_account: mint_account.clone(),
            page,
            limit,
            has_next,
            has_prev,
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

    /// Query user order information
    pub async fn query_user_orders(&self, query: UserOrderQuery) -> Result<UserOrderQueryResponse> {
        let user = &query.user;
        let mint_account = &query.mint_account;
        let page = query.page.unwrap_or(1);
        let limit = query.limit.unwrap_or(50);
        let order_by = query.order_by.unwrap_or_else(|| "start_time_desc".to_string());
        
        debug!("🔍 Querying user order information, user: {}, mint: {:?}, page: {}, limit: {}, order: {}", 
               user, mint_account, page, limit, order_by);
        
        // Build search prefix
        let prefix = if let Some(mint) = mint_account {
            format!("uo:{}:{}:", user, mint)
        } else {
            format!("uo:{}:", user)
        };
        let mut all_orders = Vec::new();
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
                Ok(order_data) => {
                    all_orders.push(order_data);
                }
                Err(e) => {
                    error!("❌ Failed to parse user order data: {}, key: {}", e, key_str);
                    continue;
                }
            }
        }
        
        // Sort by start_time
        match order_by.as_str() {
            "start_time_asc" => {
                all_orders.sort_by(|a, b| a.start_time.cmp(&b.start_time));
            }
            "start_time_desc" => {
                all_orders.sort_by(|a, b| b.start_time.cmp(&a.start_time));
            }
            _ => {
                // Default sort by start_time descending
                all_orders.sort_by(|a, b| b.start_time.cmp(&a.start_time));
            }
        }
        
        let total = all_orders.len();
        let offset = (page - 1) * limit;
        let has_prev = page > 1;
        let has_next = offset + limit < total;
        
        // Pagination
        let orders = all_orders
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        
        Ok(UserOrderQueryResponse {
            orders,
            total,
            user: user.clone(),
            mint_account: mint_account.clone(),
            page,
            limit,
            has_next,
            has_prev,
        })
    }

    /// Query kline data
    pub async fn query_kline_data(&self, query: KlineQuery) -> Result<KlineQueryResponse> {
        let mint_account = &query.mint_account;
        let interval = &query.interval;
        let page = query.page.unwrap_or(1);
        let limit = query.limit.unwrap_or(50);
        let order_by = query.order_by.unwrap_or_else(|| "time_desc".to_string());
        
        // Validate interval
        if !matches!(interval.as_str(), "s1" | "s30" | "m5") {
            return Err(anyhow::anyhow!("Invalid interval: {}, must be one of: s1, s30, m5", interval));
        }
        
        debug!("🔍 Querying kline data, mint: {}, interval: {}, page: {}, limit: {}, order: {}", 
               mint_account, interval, page, limit, order_by);
        
        // Build prefix key for the specific mint and interval
        let prefix = format!("{}:{}:", interval, mint_account);
        
        // Collect all matching kline data
        let mut all_klines = Vec::new();
        
        let iter = self.db.iterator(IteratorMode::From(prefix.as_bytes(), Direction::Forward));
        
        for item in iter {
            let (key, value) = item?;
            let key_str = String::from_utf8_lossy(&key);
            
            // Check if still matches prefix
            if !key_str.starts_with(&prefix) {
                break;
            }
            
            // Parse kline data
            match serde_json::from_slice::<KlineData>(&value) {
                Ok(kline_data) => all_klines.push(kline_data),
                Err(e) => {
                    error!("❌ Failed to parse kline data: {}, key: {}", e, key_str);
                    continue;
                }
            }
        }
        
        // Sort by time
        match order_by.as_str() {
            "time_asc" => {
                all_klines.sort_by(|a, b| a.time.cmp(&b.time));
            }
            "time_desc" => {
                all_klines.sort_by(|a, b| b.time.cmp(&a.time));
            }
            _ => {
                // Default sort by time descending (newest first)
                all_klines.sort_by(|a, b| b.time.cmp(&a.time));
            }
        }
        
        let total = all_klines.len();
        let offset = (page - 1) * limit;
        let has_prev = page > 1;
        let has_next = offset + limit < total;
        
        // Pagination
        let klines = all_klines
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        
        debug!("🔍 Retrieved {} klines for mint: {}, interval: {}", klines.len(), mint_account, interval);
        
        Ok(KlineQueryResponse {
            klines,
            total,
            page,
            limit,
            has_next,
            has_prev,
            interval: interval.clone(),
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
        let config = crate::config::Config {
            server: crate::config::ServerConfig {
                host: "localhost".to_string(),
                port: 8080,
            },
            cors: crate::config::CorsConfig {
                enabled: true,
                allow_origins: vec!["*".to_string()],
            },
            logging: crate::config::LoggingConfig {
                level: "debug".to_string(),
            },
            solana: crate::config::SolanaConfig {
                rpc_url: "http://localhost:8899".to_string(),
                ws_url: "ws://localhost:8900".to_string(),
                program_id: "JBMmrp6jhksqnxDBskkmVvWHhJLaPBjgiMHEroJbUTBZ".to_string(),
                enable_event_listener: false,
                commitment: "processed".to_string(),
                reconnect_interval: 1,
                max_reconnect_attempts: 20,
                event_buffer_size: 1000,
                event_batch_size: 100,
                ping_interval_seconds: 60,
            },
            database: crate::config::DatabaseConfig {
                rocksdb_path: temp_dir.path().to_str().unwrap().to_string(),
            },
            ipfs: crate::config::IpfsConfig {
                gateway_url: "https://crimson-binding-tarantula-509.mypinata.cloud/ipfs/".to_string(),
                request_timeout_seconds: 30,
                max_retries: 3,
                retry_delay_seconds: 5,
            },
            kline: crate::config::KlineServiceConfig {
                enable_kline_service: false,
                connection_timeout_secs: 60,
                max_subscriptions_per_client: 100,
                history_data_limit: 100,
                ping_interval_secs: 25,
                ping_timeout_secs: 60,
            },
        };
        
        let storage = EventStorage::new(&config).unwrap();
        
        let mint_detail = MintDetailData {
            mint_account: "test_mint".to_string(),
            payer: Some("test_payer".to_string()),
            curve_account: Some("test_curve".to_string()),
            pool_token_account: Some("test_pool_token".to_string()),
            pool_sol_account: Some("test_pool_sol".to_string()),
            fee_recipient: Some("test_fee_recipient".to_string()),
            base_fee_recipient: Some("test_base_fee_recipient".to_string()),
            params_account: Some("test_params_account".to_string()),
            name: Some("Test Token".to_string()),
            symbol: Some("TEST".to_string()),
            uri: Some("test_uri".to_string()),
            swap_fee: Some(100),
            borrow_fee: Some(200),
            fee_discount_flag: Some(0),
            create_timestamp: Some(Utc::now().timestamp()),
            latest_price: Some(1000000),
            latest_trade_time: Some(Utc::now().timestamp()),
            total_sol_amount: 1000,
            total_margin_sol_amount: 2000,
            total_force_liquidations: 10,
            total_close_profit: 500,
            created_by: Some("test_user".to_string()),
            last_updated_at: Some(Utc::now()),
            uri_data: None,
        };
        
        let key = storage.generate_mint_detail_key(&mint_detail.mint_account);
        let value = serde_json::to_vec(&mint_detail).unwrap();
        storage.db.put(key.as_bytes(), &value).unwrap();
        
        let query = MintDetailsQuery {
            mint_accounts: vec![mint_detail.mint_account.clone()],
        };
        
        let result = storage.query_mint_details(query).await.unwrap();
        assert_eq!(result.details.len(), 1);
        assert_eq!(result.details[0].mint_account, mint_detail.mint_account);
        assert_eq!(result.details[0].name, mint_detail.name);
        
        // Also test get_stats
        let stats = storage.get_stats().unwrap();
        assert!(stats.contains("Total Keys:"));
    }
} 
use base64::engine::Engine;
use borsh::BorshDeserialize;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use solana_sdk::pubkey::Pubkey;
use tracing::{debug, warn};
use utoipa::ToSchema;

/// Event discriminators - correct discriminators from IDL file
pub const TOKEN_CREATED_EVENT_DISCRIMINATOR: [u8; 8] = [96, 122, 113, 138, 50, 227, 149, 57];
pub const BUY_SELL_EVENT_DISCRIMINATOR: [u8; 8] = [98, 208, 120, 60, 93, 32, 19, 180];
pub const LONG_SHORT_EVENT_DISCRIMINATOR: [u8; 8] = [27, 69, 20, 116, 58, 250, 95, 220];
pub const FORCE_LIQUIDATE_EVENT_DISCRIMINATOR: [u8; 8] = [234, 196, 183, 105, 40, 26, 206, 48];
pub const FULL_CLOSE_EVENT_DISCRIMINATOR: [u8; 8] = [22, 244, 113, 245, 154, 168, 109, 139];
pub const PARTIAL_CLOSE_EVENT_DISCRIMINATOR: [u8; 8] = [133, 94, 3, 222, 24, 68, 69, 155];
pub const MILESTONE_DISCOUNT_EVENT_DISCRIMINATOR: [u8; 8] = [130, 232, 11, 37, 34, 185, 136, 128];

/// Unified enum for all Spin Pet events
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "event_type")]
pub enum SpinPetEvent {
    TokenCreated(TokenCreatedEvent),
    BuySell(BuySellEvent),
    LongShort(LongShortEvent),
    ForceLiquidate(ForceLiquidateEvent),
    FullClose(FullCloseEvent),
    PartialClose(PartialCloseEvent),
    MilestoneDiscount(MilestoneDiscountEvent),
}

/// Token creation event - exactly matches original Anchor structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenCreatedEvent {
    pub payer: String,
    pub mint_account: String,
    pub curve_account: String,
    pub pool_token_account: String,
    pub pool_sol_account: String,
    pub fee_recipient: String,
    pub base_fee_recipient: String, // 基础手续费接收账户
    pub params_account: String,     // 合作伙伴参数账户PDA地址
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub swap_fee: u16,         // 现货交易手续费
    pub borrow_fee: u16,       // 保证金交易手续费
    pub fee_discount_flag: u8, // 手续费折扣标志 0: 原价 1: 5折 2: 2.5折  3: 1.25折

    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// Buy/Sell event - exactly matches original Anchor structure
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BuySellEvent {
    pub payer: String,
    pub mint_account: String,
    pub is_buy: bool,
    pub token_amount: u64,
    pub sol_amount: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub latest_price: u128,
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// Long/Short event - exactly matches original Anchor structure
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LongShortEvent {
    pub payer: String,
    pub mint_account: String,
    pub order_pda: String,
    #[serde_as(as = "DisplayFromStr")]
    pub latest_price: u128,
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
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// Force liquidation event - exactly matches original Anchor structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ForceLiquidateEvent {
    pub payer: String,
    pub mint_account: String,
    pub order_pda: String,
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// Full close event - exactly matches original Anchor structure
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FullCloseEvent {
    pub payer: String,
    pub user_sol_account: String,
    pub mint_account: String,
    pub is_close_long: bool,
    pub final_token_amount: u64,
    pub final_sol_amount: u64,
    pub user_close_profit: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub latest_price: u128,
    pub order_pda: String,
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// Partial close event - exactly matches original Anchor structure
#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PartialCloseEvent {
    pub payer: String,
    pub user_sol_account: String,
    pub mint_account: String,
    pub is_close_long: bool,
    pub final_token_amount: u64,
    pub final_sol_amount: u64,
    pub user_close_profit: u64,
    #[serde_as(as = "DisplayFromStr")]
    pub latest_price: u128,
    pub order_pda: String,
    // Parameters for partial close order (modified values)
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
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// Milestone Discount event - exactly matches original Anchor structure
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MilestoneDiscountEvent {
    pub payer: String,
    pub mint_account: String,
    pub curve_account: String,
    pub swap_fee: u16,         // 现货交易手续费
    pub borrow_fee: u16,       // 保证金交易手续费
    pub fee_discount_flag: u8, // 手续费折扣标志 0: 原价 1: 5折 2: 2.5折  3: 1.25折
    #[schema(value_type = String)]
    pub timestamp: DateTime<Utc>,
    pub signature: String,
    pub slot: u64,
}

/// Event parser
#[derive(Clone)]
pub struct EventParser {
    #[allow(dead_code)]
    pub program_id: Pubkey,
}

impl EventParser {
    pub fn new(program_id: &str) -> anyhow::Result<Self> {
        let program_id = program_id.parse::<Pubkey>()?;
        Ok(Self { program_id })
    }

    /// Parse events with call stack tracking to capture CPI events
    pub fn parse_events_with_call_stack(
        &self,
        logs: &[String],
        signature: &str,
        slot: u64,
    ) -> anyhow::Result<Vec<SpinPetEvent>> {
        let mut events = Vec::new();
        let mut program_stack = Vec::new();
        let mut in_target_program = false;

        debug!("Starting call stack parsing for {} log lines", logs.len());

        for (i, log) in logs.iter().enumerate() {
            debug!("Processing log[{}]: {}", i, log);

            // Track program invocations
            if log.contains(" invoke [") {
                // Extract program ID from log like "Program <pubkey> invoke [depth]"
                if let Some(program_id) = Self::extract_program_id_from_log(log) {
                    program_stack.push(program_id.clone());
                    debug!(
                        "Program {} entered stack (depth: {})",
                        program_id,
                        program_stack.len()
                    );

                    // Check if our target program is now in the stack
                    if program_id == self.program_id.to_string() {
                        in_target_program = true;
                        debug!("Target program {} is now active", self.program_id);
                    }
                }
            } else if log.contains(" success") || log.contains(" failed") {
                // Program exit - pop from stack
                if let Some(exited_program) = program_stack.pop() {
                    debug!(
                        "Program {} exited stack (remaining depth: {})",
                        exited_program,
                        program_stack.len()
                    );

                    // Check if we're still in target program context
                    in_target_program = program_stack
                        .iter()
                        .any(|p| p == &self.program_id.to_string());
                    if !in_target_program {
                        debug!("Target program {} is no longer active", self.program_id);
                    }
                }
            }

            // Parse "Program data:" logs when in target program context
            if in_target_program && log.starts_with("Program data:") {
                debug!("Found Program data in target program context at log[{}]", i);

                if let Some(data_part) = log.strip_prefix("Program data: ") {
                    let data_part = data_part.trim();

                    // Base64 decode
                    match base64::engine::general_purpose::STANDARD.decode(data_part) {
                        Ok(data) => {
                            debug!("Successfully decoded Base64 data, length: {}", data.len());

                            // Parse event from data
                            match self.parse_event_data(&data, signature, slot) {
                                Ok(Some(event)) => {
                                    debug!(
                                        "Successfully parsed event from CPI context: {:?}",
                                        event
                                    );
                                    events.push(event);
                                }
                                Ok(None) => {
                                    debug!("Data didn't match any event discriminator");
                                }
                                Err(e) => {
                                    warn!("Failed to parse event data: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Base64 decoding failed: {}", e);
                        }
                    }
                }
            }
        }

        debug!("Call stack parsing complete. Found {} events", events.len());
        Ok(events)
    }

    /// Extract program ID from invoke log line
    fn extract_program_id_from_log(log: &str) -> Option<String> {
        // Log format: "Program <pubkey> invoke [depth]"
        if let Some(start) = log.find("Program ") {
            let after_program = &log[start + 8..];
            if let Some(end) = after_program.find(" invoke") {
                return Some(after_program[..end].to_string());
            }
        }
        None
    }

    /// Parse event data
    fn parse_event_data(
        &self,
        data: &[u8],
        signature: &str,
        slot: u64,
    ) -> anyhow::Result<Option<SpinPetEvent>> {
        debug!(
            "🔍 Starting to parse event data, total length: {}",
            data.len()
        );

        if data.len() < 8 {
            warn!("⚠️ Data length insufficient, need at least 8 bytes for discriminator, actual length: {}", data.len());
            return Ok(None);
        }

        let discriminator = &data[0..8];
        let event_data = &data[8..];
        let timestamp = Utc::now();

        debug!("🔍 Parsed discriminator: {:?}", discriminator);
        debug!("📊 Event data length: {}", event_data.len());

        // Match using correct discriminators from IDL file
        match discriminator {
            d if d == TOKEN_CREATED_EVENT_DISCRIMINATOR => {
                debug!("🪙 Matched TokenCreatedEvent, discriminator: {:?}", d);
                let event =
                    self.parse_token_created_event(event_data, signature, slot, timestamp)?;
                Ok(Some(SpinPetEvent::TokenCreated(event)))
            }
            d if d == BUY_SELL_EVENT_DISCRIMINATOR => {
                debug!("💰 Matched BuySellEvent, discriminator: {:?}", d);
                let event = self.parse_buy_sell_event(event_data, signature, slot, timestamp)?;
                Ok(Some(SpinPetEvent::BuySell(event)))
            }
            d if d == LONG_SHORT_EVENT_DISCRIMINATOR => {
                debug!("📈 Matched LongShortEvent, discriminator: {:?}", d);
                let event = self.parse_long_short_event(event_data, signature, slot, timestamp)?;
                Ok(Some(SpinPetEvent::LongShort(event)))
            }
            d if d == FORCE_LIQUIDATE_EVENT_DISCRIMINATOR => {
                debug!("⚠️ Matched ForceLiquidateEvent, discriminator: {:?}", d);
                let event =
                    self.parse_force_liquidate_event(event_data, signature, slot, timestamp)?;
                Ok(Some(SpinPetEvent::ForceLiquidate(event)))
            }
            d if d == FULL_CLOSE_EVENT_DISCRIMINATOR => {
                debug!("🔒 Matched FullCloseEvent, discriminator: {:?}", d);
                let event = self.parse_full_close_event(event_data, signature, slot, timestamp)?;
                Ok(Some(SpinPetEvent::FullClose(event)))
            }
            d if d == PARTIAL_CLOSE_EVENT_DISCRIMINATOR => {
                debug!("🔓 Matched PartialCloseEvent, discriminator: {:?}", d);
                let event =
                    self.parse_partial_close_event(event_data, signature, slot, timestamp)?;
                Ok(Some(SpinPetEvent::PartialClose(event)))
            }
            d if d == MILESTONE_DISCOUNT_EVENT_DISCRIMINATOR => {
                debug!("💲 Matched MilestoneDiscountEvent, discriminator: {:?}", d);
                let event =
                    self.parse_milestone_discount_event(event_data, signature, slot, timestamp)?;
                Ok(Some(SpinPetEvent::MilestoneDiscount(event)))
            }
            _ => {
                debug!("❓ Unknown event discriminator: {:?}", discriminator);
                Ok(None)
            }
        }
    }

    /// Parse TokenCreatedEvent
    fn parse_token_created_event(
        &self,
        data: &[u8],
        signature: &str,
        slot: u64,
        timestamp: DateTime<Utc>,
    ) -> anyhow::Result<TokenCreatedEvent> {
        debug!(
            "🪙 Starting to parse TokenCreatedEvent, data length: {}",
            data.len()
        );

        if data.len() < 261 {
            return Err(anyhow::anyhow!(
                "TokenCreatedEvent data length insufficient, need at least 261 bytes, actual: {}",
                data.len()
            ));
        }

        debug!("🔍 Parsing payer (0..32)");
        let payer = Pubkey::try_from_slice(&data[0..32])
            .map_err(|e| anyhow::anyhow!("Failed to parse payer: {}", e))?;
        debug!("✅ payer: {}", payer);

        debug!("🔍 Parsing mint_account (32..64)");
        let mint_account = Pubkey::try_from_slice(&data[32..64])
            .map_err(|e| anyhow::anyhow!("Failed to parse mint_account: {}", e))?;
        debug!("✅ mint_account: {}", mint_account);

        debug!("🔍 Parsing curve_account (64..96)");
        let curve_account = Pubkey::try_from_slice(&data[64..96])
            .map_err(|e| anyhow::anyhow!("Failed to parse curve_account: {}", e))?;
        debug!("✅ curve_account: {}", curve_account);

        debug!("🔍 Parsing pool_token_account (96..128)");
        let pool_token_account = Pubkey::try_from_slice(&data[96..128])
            .map_err(|e| anyhow::anyhow!("Failed to parse pool_token_account: {}", e))?;
        debug!("✅ pool_token_account: {}", pool_token_account);

        debug!("🔍 Parsing pool_sol_account (128..160)");
        let pool_sol_account = Pubkey::try_from_slice(&data[128..160])
            .map_err(|e| anyhow::anyhow!("Failed to parse pool_sol_account: {}", e))?;
        debug!("✅ pool_sol_account: {}", pool_sol_account);

        debug!("🔍 Parsing fee_recipient (160..192)");
        let fee_recipient = Pubkey::try_from_slice(&data[160..192])
            .map_err(|e| anyhow::anyhow!("Failed to parse fee_recipient: {}", e))?;
        debug!("✅ fee_recipient: {}", fee_recipient);

        debug!("🔍 Parsing base_fee_recipient (192..224)");
        let base_fee_recipient = Pubkey::try_from_slice(&data[192..224])
            .map_err(|e| anyhow::anyhow!("Failed to parse base_fee_recipient: {}", e))?;
        debug!("✅ base_fee_recipient: {}", base_fee_recipient);

        debug!("🔍 Parsing params_account (224..256)");
        let params_account = Pubkey::try_from_slice(&data[224..256])
            .map_err(|e| anyhow::anyhow!("Failed to parse params_account: {}", e))?;
        debug!("✅ params_account: {}", params_account);

        debug!("🔍 Parsing swap_fee (256..258)");
        let swap_fee = u16::from_le_bytes(
            data[256..258]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse swap_fee: {}", e))?,
        );
        debug!("✅ swap_fee: {}", swap_fee);

        debug!("🔍 Parsing borrow_fee (258..260)");
        let borrow_fee = u16::from_le_bytes(
            data[258..260]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse borrow_fee: {}", e))?,
        );
        debug!("✅ borrow_fee: {}", borrow_fee);

        debug!("🔍 Parsing fee_discount_flag (260)");
        let fee_discount_flag = data[260];
        debug!("✅ fee_discount_flag: {}", fee_discount_flag);

        // Parse string fields (Borsh format: 4-byte length + string data)
        let mut offset = 261;
        debug!(
            "🔍 Starting to parse string fields, starting offset: {}",
            offset
        );

        // Parse name
        if offset + 4 > data.len() {
            return Err(anyhow::anyhow!(
                "Data insufficient to read name length, offset: {}, data length: {}",
                offset,
                data.len()
            ));
        }
        let name_len = u32::from_le_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse name length: {}", e))?,
        ) as usize;
        offset += 4;
        debug!("🔍 name length: {}", name_len);

        if offset + name_len > data.len() {
            return Err(anyhow::anyhow!(
                "Data insufficient to read name data, need: {}, remaining: {}",
                name_len,
                data.len() - offset
            ));
        }
        let name = String::from_utf8(data[offset..offset + name_len].to_vec())
            .map_err(|e| anyhow::anyhow!("Failed to parse name string: {}", e))?;
        offset += name_len;
        debug!("✅ name: {}", name);

        // Parse symbol
        if offset + 4 > data.len() {
            return Err(anyhow::anyhow!(
                "Data insufficient to read symbol length, offset: {}, data length: {}",
                offset,
                data.len()
            ));
        }
        let symbol_len = u32::from_le_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse symbol length: {}", e))?,
        ) as usize;
        offset += 4;
        debug!("🔍 symbol length: {}", symbol_len);

        if offset + symbol_len > data.len() {
            return Err(anyhow::anyhow!(
                "Data insufficient to read symbol data, need: {}, remaining: {}",
                symbol_len,
                data.len() - offset
            ));
        }
        let symbol = String::from_utf8(data[offset..offset + symbol_len].to_vec())
            .map_err(|e| anyhow::anyhow!("Failed to parse symbol string: {}", e))?;
        offset += symbol_len;
        debug!("✅ symbol: {}", symbol);

        // Parse uri
        if offset + 4 > data.len() {
            return Err(anyhow::anyhow!(
                "Data insufficient to read uri length, offset: {}, data length: {}",
                offset,
                data.len()
            ));
        }
        let uri_len = u32::from_le_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse uri length: {}", e))?,
        ) as usize;
        offset += 4;
        debug!("🔍 uri length: {}", uri_len);

        if offset + uri_len > data.len() {
            return Err(anyhow::anyhow!(
                "Data insufficient to read uri data, need: {}, remaining: {}",
                uri_len,
                data.len() - offset
            ));
        }
        let uri = String::from_utf8(data[offset..offset + uri_len].to_vec())
            .map_err(|e| anyhow::anyhow!("Failed to parse uri string: {}", e))?;
        debug!("✅ uri: {}", uri);

        debug!("🎉 TokenCreatedEvent parsed");
        Ok(TokenCreatedEvent {
            payer: payer.to_string(),
            mint_account: mint_account.to_string(),
            curve_account: curve_account.to_string(),
            pool_token_account: pool_token_account.to_string(),
            pool_sol_account: pool_sol_account.to_string(),
            fee_recipient: fee_recipient.to_string(),
            base_fee_recipient: base_fee_recipient.to_string(),
            params_account: params_account.to_string(),
            name,
            symbol,
            uri,
            swap_fee,
            borrow_fee,
            fee_discount_flag,
            timestamp,
            signature: signature.to_string(),
            slot,
        })
    }

    /// Parse BuySellEvent
    fn parse_buy_sell_event(
        &self,
        data: &[u8],
        signature: &str,
        slot: u64,
        timestamp: DateTime<Utc>,
    ) -> anyhow::Result<BuySellEvent> {
        debug!(
            "💰 Starting to parse BuySellEvent, data length: {}",
            data.len()
        );

        if data.len() < 97 {
            return Err(anyhow::anyhow!(
                "BuySellEvent data length insufficient, need at least 97 bytes, actual: {}",
                data.len()
            ));
        }

        debug!("🔍 Parsing payer (0..32)");
        let payer = Pubkey::try_from_slice(&data[0..32])
            .map_err(|e| anyhow::anyhow!("Failed to parse payer: {}", e))?;
        debug!("✅ payer: {}", payer);

        debug!("🔍 Parsing mint_account (32..64)");
        let mint_account = Pubkey::try_from_slice(&data[32..64])
            .map_err(|e| anyhow::anyhow!("Failed to parse mint_account: {}", e))?;
        debug!("✅ mint_account: {}", mint_account);

        debug!("🔍 Parsing is_buy (64)");
        let is_buy = data[64] != 0;
        debug!("✅ is_buy: {}", is_buy);

        debug!("🔍 Parsing token_amount (65..73)");
        let token_amount = u64::from_le_bytes(
            data[65..73]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse token_amount: {}", e))?,
        );
        debug!("✅ token_amount: {}", token_amount);

        debug!("🔍 Parsing sol_amount (73..81)");
        let sol_amount = u64::from_le_bytes(
            data[73..81]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse sol_amount: {}", e))?,
        );
        debug!("✅ sol_amount: {}", sol_amount);

        debug!("🔍 Parsing latest_price (81..97)");
        let latest_price = u128::from_le_bytes(
            data[81..97]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse latest_price: {}", e))?,
        );
        debug!("✅ latest_price: {}", latest_price);

        debug!("🎉 BuySellEvent parsed");
        Ok(BuySellEvent {
            payer: payer.to_string(),
            mint_account: mint_account.to_string(),
            is_buy,
            token_amount,
            sol_amount,
            latest_price,
            timestamp,
            signature: signature.to_string(),
            slot,
        })
    }

    /// Parse LongShortEvent
    fn parse_long_short_event(
        &self,
        data: &[u8],
        signature: &str,
        slot: u64,
        timestamp: DateTime<Utc>,
    ) -> anyhow::Result<LongShortEvent> {
        debug!(
            "📈 Starting to parse LongShortEvent, data length: {}",
            data.len()
        );

        if data.len() < 259 {
            return Err(anyhow::anyhow!(
                "LongShortEvent data length insufficient, need at least 259 bytes, actual: {}",
                data.len()
            ));
        }

        debug!("🔍 Parsing payer (0..32)");
        let payer = Pubkey::try_from_slice(&data[0..32])
            .map_err(|e| anyhow::anyhow!("Failed to parse payer: {}", e))?;
        debug!("✅ payer: {}", payer);

        debug!("🔍 Parsing mint_account (32..64)");
        let mint_account = Pubkey::try_from_slice(&data[32..64])
            .map_err(|e| anyhow::anyhow!("Failed to parse mint_account: {}", e))?;
        debug!("✅ mint_account: {}", mint_account);

        debug!("🔍 Parsing order_pda (64..96)");
        let order_pda = Pubkey::try_from_slice(&data[64..96])
            .map_err(|e| anyhow::anyhow!("Failed to parse order_pda: {}", e))?;
        debug!("✅ order_pda: {}", order_pda);

        debug!("🔍 Parsing latest_price (96..112)");
        let latest_price = u128::from_le_bytes(
            data[96..112]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse latest_price: {}", e))?,
        );
        debug!("✅ latest_price: {}", latest_price);

        debug!("🔍 Parsing order_type (112)");
        let order_type = data[112];
        debug!("✅ order_type: {}", order_type);

        debug!("🔍 Parsing mint (113..145)");
        let mint = Pubkey::try_from_slice(&data[113..145])
            .map_err(|e| anyhow::anyhow!("Failed to parse mint: {}", e))?;
        debug!("✅ mint: {}", mint);

        debug!("🔍 Parsing user (145..177)");
        let user = Pubkey::try_from_slice(&data[145..177])
            .map_err(|e| anyhow::anyhow!("Failed to parse user: {}", e))?;
        debug!("✅ user: {}", user);

        debug!("🔍 Parsing lock_lp_start_price (177..193)");
        let lock_lp_start_price = u128::from_le_bytes(
            data[177..193]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse lock_lp_start_price: {}", e))?,
        );
        debug!("✅ lock_lp_start_price: {}", lock_lp_start_price);

        debug!("🔍 Parsing lock_lp_end_price (193..209)");
        let lock_lp_end_price = u128::from_le_bytes(
            data[193..209]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse lock_lp_end_price: {}", e))?,
        );
        debug!("✅ lock_lp_end_price: {}", lock_lp_end_price);

        debug!("🔍 Parsing lock_lp_sol_amount (209..217)");
        let lock_lp_sol_amount = u64::from_le_bytes(
            data[209..217]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse lock_lp_sol_amount: {}", e))?,
        );
        debug!("✅ lock_lp_sol_amount: {}", lock_lp_sol_amount);

        debug!("🔍 Parsing lock_lp_token_amount (217..225)");
        let lock_lp_token_amount = u64::from_le_bytes(
            data[217..225]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse lock_lp_token_amount: {}", e))?,
        );
        debug!("✅ lock_lp_token_amount: {}", lock_lp_token_amount);

        debug!("🔍 Parsing start_time (225..229)");
        let start_time = u32::from_le_bytes(
            data[225..229]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse start_time: {}", e))?,
        );
        debug!("✅ start_time: {}", start_time);

        debug!("🔍 Parsing end_time (229..233)");
        let end_time = u32::from_le_bytes(
            data[229..233]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse end_time: {}", e))?,
        );
        debug!("✅ end_time: {}", end_time);

        debug!("🔍 Parsing margin_sol_amount (233..241)");
        let margin_sol_amount = u64::from_le_bytes(
            data[233..241]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse margin_sol_amount: {}", e))?,
        );
        debug!("✅ margin_sol_amount: {}", margin_sol_amount);

        debug!("🔍 Parsing borrow_amount (241..249)");
        let borrow_amount = u64::from_le_bytes(
            data[241..249]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse borrow_amount: {}", e))?,
        );
        debug!("✅ borrow_amount: {}", borrow_amount);

        debug!("🔍 Parsing position_asset_amount (249..257)");
        let position_asset_amount = u64::from_le_bytes(
            data[249..257]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse position_asset_amount: {}", e))?,
        );
        debug!("✅ position_asset_amount: {}", position_asset_amount);

        debug!("🔍 Parsing borrow_fee (257..259)");
        let borrow_fee = u16::from_le_bytes(
            data[257..259]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse borrow_fee: {}", e))?,
        );
        debug!("✅ borrow_fee: {}", borrow_fee);

        debug!("🎉 LongShortEvent parsed");
        Ok(LongShortEvent {
            payer: payer.to_string(),
            mint_account: mint_account.to_string(),
            order_pda: order_pda.to_string(),
            latest_price,
            order_type,
            mint: mint.to_string(),
            user: user.to_string(),
            lock_lp_start_price,
            lock_lp_end_price,
            lock_lp_sol_amount,
            lock_lp_token_amount,
            start_time,
            end_time,
            margin_sol_amount,
            borrow_amount,
            position_asset_amount,
            borrow_fee,
            timestamp,
            signature: signature.to_string(),
            slot,
        })
    }

    /// Parse ForceLiquidateEvent
    fn parse_force_liquidate_event(
        &self,
        data: &[u8],
        signature: &str,
        slot: u64,
        timestamp: DateTime<Utc>,
    ) -> anyhow::Result<ForceLiquidateEvent> {
        debug!(
            "⚠️ Starting to parse ForceLiquidateEvent, data length: {}",
            data.len()
        );

        if data.len() < 96 {
            return Err(anyhow::anyhow!(
                "ForceLiquidateEvent data length insufficient, need at least 96 bytes, actual: {}",
                data.len()
            ));
        }

        debug!("🔍 Parsing payer (0..32)");
        let payer = Pubkey::try_from_slice(&data[0..32])
            .map_err(|e| anyhow::anyhow!("Failed to parse payer: {}", e))?;
        debug!("✅ payer: {}", payer);

        debug!("🔍 Parsing mint_account (32..64)");
        let mint_account = Pubkey::try_from_slice(&data[32..64])
            .map_err(|e| anyhow::anyhow!("Failed to parse mint_account: {}", e))?;
        debug!("✅ mint_account: {}", mint_account);

        debug!("🔍 Parsing order_pda (64..96)");
        let order_pda = Pubkey::try_from_slice(&data[64..96])
            .map_err(|e| anyhow::anyhow!("Failed to parse order_pda: {}", e))?;
        debug!("✅ order_pda: {}", order_pda);

        debug!("🎉 ForceLiquidateEvent parsed");
        Ok(ForceLiquidateEvent {
            payer: payer.to_string(),
            mint_account: mint_account.to_string(),
            order_pda: order_pda.to_string(),
            timestamp,
            signature: signature.to_string(),
            slot,
        })
    }

    /// Parse FullCloseEvent
    fn parse_full_close_event(
        &self,
        data: &[u8],
        signature: &str,
        slot: u64,
        timestamp: DateTime<Utc>,
    ) -> anyhow::Result<FullCloseEvent> {
        debug!(
            "🔒 Starting to parse FullCloseEvent, data length: {}",
            data.len()
        );

        if data.len() < 169 {
            return Err(anyhow::anyhow!(
                "FullCloseEvent data length insufficient, need at least 169 bytes, actual: {}",
                data.len()
            ));
        }

        debug!("🔍 Parsing payer (0..32)");
        let payer = Pubkey::try_from_slice(&data[0..32])
            .map_err(|e| anyhow::anyhow!("Failed to parse payer: {}", e))?;
        debug!("✅ payer: {}", payer);

        debug!("🔍 Parsing user_sol_account (32..64)");
        let user_sol_account = Pubkey::try_from_slice(&data[32..64])
            .map_err(|e| anyhow::anyhow!("Failed to parse user_sol_account: {}", e))?;
        debug!("✅ user_sol_account: {}", user_sol_account);

        debug!("🔍 Parsing mint_account (64..96)");
        let mint_account = Pubkey::try_from_slice(&data[64..96])
            .map_err(|e| anyhow::anyhow!("Failed to parse mint_account: {}", e))?;
        debug!("✅ mint_account: {}", mint_account);

        debug!("🔍 Parsing is_close_long (96)");
        let is_close_long = data[96] != 0;
        debug!("✅ is_close_long: {}", is_close_long);

        debug!("🔍 Parsing final_token_amount (97..105)");
        let final_token_amount = u64::from_le_bytes(
            data[97..105]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse final_token_amount: {}", e))?,
        );
        debug!("✅ final_token_amount: {}", final_token_amount);

        debug!("🔍 Parsing final_sol_amount (105..113)");
        let final_sol_amount = u64::from_le_bytes(
            data[105..113]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse final_sol_amount: {}", e))?,
        );
        debug!("✅ final_sol_amount: {}", final_sol_amount);

        debug!("🔍 Parsing user_close_profit (113..121)");
        let user_close_profit = u64::from_le_bytes(
            data[113..121]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse user_close_profit: {}", e))?,
        );
        debug!("✅ user_close_profit: {}", user_close_profit);

        debug!("🔍 Parsing latest_price (121..137)");
        let latest_price = u128::from_le_bytes(
            data[121..137]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse latest_price: {}", e))?,
        );
        debug!("✅ latest_price: {}", latest_price);

        debug!("🔍 Parsing order_pda (137..169)");
        let order_pda = Pubkey::try_from_slice(&data[137..169])
            .map_err(|e| anyhow::anyhow!("Failed to parse order_pda: {}", e))?;
        debug!("✅ order_pda: {}", order_pda);

        debug!("🎉 FullCloseEvent parsed");
        Ok(FullCloseEvent {
            payer: payer.to_string(),
            user_sol_account: user_sol_account.to_string(),
            mint_account: mint_account.to_string(),
            is_close_long,
            final_token_amount,
            final_sol_amount,
            user_close_profit,
            latest_price,
            order_pda: order_pda.to_string(),
            timestamp,
            signature: signature.to_string(),
            slot,
        })
    }

    /// Parse PartialCloseEvent
    fn parse_partial_close_event(
        &self,
        data: &[u8],
        signature: &str,
        slot: u64,
        timestamp: DateTime<Utc>,
    ) -> anyhow::Result<PartialCloseEvent> {
        debug!(
            "🔓 Starting to parse PartialCloseEvent, data length: {}",
            data.len()
        );

        if data.len() < 316 {
            return Err(anyhow::anyhow!(
                "PartialCloseEvent data length insufficient, need at least 316 bytes, actual: {}",
                data.len()
            ));
        }

        debug!("🔍 Parsing payer (0..32)");
        let payer = Pubkey::try_from_slice(&data[0..32])
            .map_err(|e| anyhow::anyhow!("Failed to parse payer: {}", e))?;
        debug!("✅ payer: {}", payer);

        debug!("🔍 Parsing user_sol_account (32..64)");
        let user_sol_account = Pubkey::try_from_slice(&data[32..64])
            .map_err(|e| anyhow::anyhow!("Failed to parse user_sol_account: {}", e))?;
        debug!("✅ user_sol_account: {}", user_sol_account);

        debug!("🔍 Parsing mint_account (64..96)");
        let mint_account = Pubkey::try_from_slice(&data[64..96])
            .map_err(|e| anyhow::anyhow!("Failed to parse mint_account: {}", e))?;
        debug!("✅ mint_account: {}", mint_account);

        debug!("🔍 Parsing is_close_long (96)");
        let is_close_long = data[96] != 0;
        debug!("✅ is_close_long: {}", is_close_long);

        debug!("🔍 Parsing final_token_amount (97..105)");
        let final_token_amount = u64::from_le_bytes(
            data[97..105]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse final_token_amount: {}", e))?,
        );
        debug!("✅ final_token_amount: {}", final_token_amount);

        debug!("🔍 Parsing final_sol_amount (105..113)");
        let final_sol_amount = u64::from_le_bytes(
            data[105..113]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse final_sol_amount: {}", e))?,
        );
        debug!("✅ final_sol_amount: {}", final_sol_amount);

        debug!("🔍 Parsing user_close_profit (113..121)");
        let user_close_profit = u64::from_le_bytes(
            data[113..121]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse user_close_profit: {}", e))?,
        );
        debug!("✅ user_close_profit: {}", user_close_profit);

        debug!("🔍 Parsing latest_price (121..137)");
        let latest_price = u128::from_le_bytes(
            data[121..137]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse latest_price: {}", e))?,
        );
        debug!("✅ latest_price: {}", latest_price);

        debug!("🔍 Parsing order_pda (137..169)");
        let order_pda = Pubkey::try_from_slice(&data[137..169])
            .map_err(|e| anyhow::anyhow!("Failed to parse order_pda: {}", e))?;
        debug!("✅ order_pda: {}", order_pda);

        debug!("🔍 Parsing order_type (169)");
        let order_type = data[169];
        debug!("✅ order_type: {}", order_type);

        debug!("🔍 Parsing mint (170..202)");
        let mint = Pubkey::try_from_slice(&data[170..202])
            .map_err(|e| anyhow::anyhow!("Failed to parse mint: {}", e))?;
        debug!("✅ mint: {}", mint);

        debug!("🔍 Parsing user (202..234)");
        let user = Pubkey::try_from_slice(&data[202..234])
            .map_err(|e| anyhow::anyhow!("Failed to parse user: {}", e))?;
        debug!("✅ user: {}", user);

        debug!("🔍 Parsing lock_lp_start_price (234..250)");
        let lock_lp_start_price = u128::from_le_bytes(
            data[234..250]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse lock_lp_start_price: {}", e))?,
        );
        debug!("✅ lock_lp_start_price: {}", lock_lp_start_price);

        debug!("🔍 Parsing lock_lp_end_price (250..266)");
        let lock_lp_end_price = u128::from_le_bytes(
            data[250..266]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse lock_lp_end_price: {}", e))?,
        );
        debug!("✅ lock_lp_end_price: {}", lock_lp_end_price);

        debug!("🔍 Parsing lock_lp_sol_amount (266..274)");
        let lock_lp_sol_amount = u64::from_le_bytes(
            data[266..274]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse lock_lp_sol_amount: {}", e))?,
        );
        debug!("✅ lock_lp_sol_amount: {}", lock_lp_sol_amount);

        debug!("🔍 Parsing lock_lp_token_amount (274..282)");
        let lock_lp_token_amount = u64::from_le_bytes(
            data[274..282]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse lock_lp_token_amount: {}", e))?,
        );
        debug!("✅ lock_lp_token_amount: {}", lock_lp_token_amount);

        debug!("🔍 Parsing start_time (282..286)");
        let start_time = u32::from_le_bytes(
            data[282..286]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse start_time: {}", e))?,
        );
        debug!("✅ start_time: {}", start_time);

        debug!("🔍 Parsing end_time (286..290)");
        let end_time = u32::from_le_bytes(
            data[286..290]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse end_time: {}", e))?,
        );
        debug!("✅ end_time: {}", end_time);

        debug!("🔍 Parsing margin_sol_amount (290..298)");
        let margin_sol_amount = u64::from_le_bytes(
            data[290..298]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse margin_sol_amount: {}", e))?,
        );
        debug!("✅ margin_sol_amount: {}", margin_sol_amount);

        debug!("🔍 Parsing borrow_amount (298..306)");
        let borrow_amount = u64::from_le_bytes(
            data[298..306]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse borrow_amount: {}", e))?,
        );
        debug!("✅ borrow_amount: {}", borrow_amount);

        debug!("🔍 Parsing position_asset_amount (306..314)");
        let position_asset_amount = u64::from_le_bytes(
            data[306..314]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse position_asset_amount: {}", e))?,
        );
        debug!("✅ position_asset_amount: {}", position_asset_amount);

        debug!("🔍 Parsing borrow_fee (314..316)");
        let borrow_fee = u16::from_le_bytes(
            data[314..316]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse borrow_fee: {}", e))?,
        );
        debug!("✅ borrow_fee: {}", borrow_fee);

        debug!("🎉 PartialCloseEvent parsed");
        Ok(PartialCloseEvent {
            payer: payer.to_string(),
            user_sol_account: user_sol_account.to_string(),
            mint_account: mint_account.to_string(),
            is_close_long,
            final_token_amount,
            final_sol_amount,
            user_close_profit,
            latest_price,
            order_pda: order_pda.to_string(),
            order_type,
            mint: mint.to_string(),
            user: user.to_string(),
            lock_lp_start_price,
            lock_lp_end_price,
            lock_lp_sol_amount,
            lock_lp_token_amount,
            start_time,
            end_time,
            margin_sol_amount,
            borrow_amount,
            position_asset_amount,
            borrow_fee,
            timestamp,
            signature: signature.to_string(),
            slot,
        })
    }

    /// Parse MilestoneDiscountEvent
    fn parse_milestone_discount_event(
        &self,
        data: &[u8],
        signature: &str,
        slot: u64,
        timestamp: DateTime<Utc>,
    ) -> anyhow::Result<MilestoneDiscountEvent> {
        debug!(
            "💲 Starting to parse MilestoneDiscountEvent, data length: {}",
            data.len()
        );

        if data.len() < 99 {
            return Err(anyhow::anyhow!("MilestoneDiscountEvent data length insufficient, need at least 99 bytes, actual: {}", data.len()));
        }

        debug!("🔍 Parsing payer (0..32)");
        let payer = Pubkey::try_from_slice(&data[0..32])
            .map_err(|e| anyhow::anyhow!("Failed to parse payer: {}", e))?;
        debug!("✅ payer: {}", payer);

        debug!("🔍 Parsing mint_account (32..64)");
        let mint_account = Pubkey::try_from_slice(&data[32..64])
            .map_err(|e| anyhow::anyhow!("Failed to parse mint_account: {}", e))?;
        debug!("✅ mint_account: {}", mint_account);

        debug!("🔍 Parsing curve_account (64..96)");
        let curve_account = Pubkey::try_from_slice(&data[64..96])
            .map_err(|e| anyhow::anyhow!("Failed to parse curve_account: {}", e))?;
        debug!("✅ curve_account: {}", curve_account);

        debug!("🔍 Parsing swap_fee (96..98)");
        let swap_fee = u16::from_le_bytes(
            data[96..98]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse swap_fee: {}", e))?,
        );
        debug!("✅ swap_fee: {}", swap_fee);

        debug!("🔍 Parsing borrow_fee (98..100)");
        let borrow_fee = u16::from_le_bytes(
            data[98..100]
                .try_into()
                .map_err(|e| anyhow::anyhow!("Failed to parse borrow_fee: {}", e))?,
        );
        debug!("✅ borrow_fee: {}", borrow_fee);

        debug!("🔍 Parsing fee_discount_flag (100)");
        let fee_discount_flag = data[100];
        debug!("✅ fee_discount_flag: {}", fee_discount_flag);

        debug!("🎉 MilestoneDiscountEvent parsed");
        Ok(MilestoneDiscountEvent {
            payer: payer.to_string(),
            mint_account: mint_account.to_string(),
            curve_account: curve_account.to_string(),
            swap_fee,
            borrow_fee,
            fee_discount_flag,
            timestamp,
            signature: signature.to_string(),
            slot,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_discriminator_constants() {
        // Test discriminator constants from IDL file
        assert_eq!(TOKEN_CREATED_EVENT_DISCRIMINATOR.len(), 8);
        assert_eq!(BUY_SELL_EVENT_DISCRIMINATOR.len(), 8);
        assert_eq!(LONG_SHORT_EVENT_DISCRIMINATOR.len(), 8);
        assert_eq!(FORCE_LIQUIDATE_EVENT_DISCRIMINATOR.len(), 8);
        assert_eq!(FULL_CLOSE_EVENT_DISCRIMINATOR.len(), 8);
        assert_eq!(PARTIAL_CLOSE_EVENT_DISCRIMINATOR.len(), 8);
        assert_eq!(MILESTONE_DISCOUNT_EVENT_DISCRIMINATOR.len(), 8);

        // Test that each discriminator is unique
        let discriminators = vec![
            TOKEN_CREATED_EVENT_DISCRIMINATOR,
            BUY_SELL_EVENT_DISCRIMINATOR,
            LONG_SHORT_EVENT_DISCRIMINATOR,
            FORCE_LIQUIDATE_EVENT_DISCRIMINATOR,
            FULL_CLOSE_EVENT_DISCRIMINATOR,
            PARTIAL_CLOSE_EVENT_DISCRIMINATOR,
            MILESTONE_DISCOUNT_EVENT_DISCRIMINATOR,
        ];

        for (i, disc1) in discriminators.iter().enumerate() {
            for (j, disc2) in discriminators.iter().enumerate() {
                if i != j {
                    assert_ne!(disc1, disc2, "Discriminators should not be the same");
                }
            }
        }
    }
}

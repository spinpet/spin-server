use super::events::{EventParser, SpinPetEvent};
use super::client::SolanaClient;
use crate::config::SolanaConfig;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{info, error, debug, warn};
use std::sync::Arc;
use std::collections::HashSet;
use async_trait::async_trait;
use uuid::Uuid;
use ezsockets::{ClientConfig, CloseFrame, Error};
use url::Url;

/// Event listener trait
#[async_trait]
pub trait EventListener {
    async fn start(&mut self) -> anyhow::Result<()>;
    #[allow(dead_code)]
    async fn stop(&mut self) -> anyhow::Result<()>;
    fn is_running(&self) -> bool;
}

/// Event handler trait
#[async_trait]
pub trait EventHandler: Send + Sync {
    async fn handle_event(&self, event: SpinPetEvent) -> anyhow::Result<()>;
}

/// Default event handler - simply print events
pub struct DefaultEventHandler;

#[async_trait]
impl EventHandler for DefaultEventHandler {
    async fn handle_event(&self, event: SpinPetEvent) -> anyhow::Result<()> {
        match event {
            SpinPetEvent::TokenCreated(e) => {
                info!("🪙 Token creation event: {} created token {}", e.payer, e.mint_account);
                info!("   - Token name: {}", e.name);
                info!("   - Token symbol: {}", e.symbol);
                info!("   - Curve account: {}", e.curve_account);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::BuySell(e) => {
                let action = if e.is_buy { "bought" } else { "sold" };
                info!("💰 Trade event: {} {} token {} (token amount: {}, SOL amount: {})", 
                      e.payer, action, e.mint_account, e.token_amount, e.sol_amount);
                info!("   - Latest price: {}", e.latest_price);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::LongShort(e) => {
                let direction = if e.order_type == 1 { "long" } else { "short" };
                info!("📈 Long/Short event: {} went {} on token {} (order PDA: {})", 
                      e.payer, direction, e.mint_account, e.order_pda);
                info!("   - User: {}", e.user);
                info!("   - Margin SOL amount: {}", e.margin_sol_amount);
                info!("   - Borrow amount: {}", e.borrow_amount);
                info!("   - Lock LP start price: {}", e.lock_lp_start_price);
                info!("   - Lock LP end price: {}", e.lock_lp_end_price);
                info!("   - Start time: {}", e.start_time);
                info!("   - End time: {}", e.end_time);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::ForceLiquidate(e) => {
                info!("⚠️ Force liquidation event: {} liquidated order {} on token {}", 
                      e.payer, e.order_pda, e.mint_account);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::FullClose(e) => {
                let direction = if e.is_close_long { "long" } else { "short" };
                info!("🔒 Full close event: {} closed {} order {} on token {} (profit: {})", 
                      e.payer, direction, e.order_pda, e.mint_account, e.user_close_profit);
                info!("   - Final token amount: {}", e.final_token_amount);
                info!("   - Final SOL amount: {}", e.final_sol_amount);
                info!("   - Latest price: {}", e.latest_price);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::PartialClose(e) => {
                let direction = if e.is_close_long { "long" } else { "short" };
                info!("🔓 Partial close event: {} partially closed {} order {} on token {} (profit: {})", 
                      e.payer, direction, e.order_pda, e.mint_account, e.user_close_profit);
                info!("   - Final token amount: {}", e.final_token_amount);
                info!("   - Final SOL amount: {}", e.final_sol_amount);
                info!("   - Latest price: {}", e.latest_price);
                info!("   - Remaining position: {}", e.position_asset_amount);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::MilestoneDiscount(e) => {
                info!("💲 Milestone discount event: {} updated fees for token {}", 
                      e.payer, e.mint_account);
                info!("   - Swap fee: {}", e.swap_fee);
                info!("   - Borrow fee: {}", e.borrow_fee);
                info!("   - Fee discount flag: {} (0: 原价, 1: 5折, 2: 2.5折, 3: 1.25折)", e.fee_discount_flag);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
        }
        Ok(())
    }
}

/// WebSocket client handler for Solana events
pub struct SolanaWebSocketClient {
    config: SolanaConfig,
    client: Arc<SolanaClient>,
    event_parser: EventParser,
    event_handler: Arc<dyn EventHandler>,
    event_sender: Option<mpsc::UnboundedSender<SpinPetEvent>>,
    processed_signatures: Arc<tokio::sync::RwLock<HashSet<String>>>,
    socket: Option<ezsockets::Socket<Self>>,
    reconnect_attempts: Arc<tokio::sync::RwLock<u32>>,
    is_connected: Arc<tokio::sync::RwLock<bool>>,
}

impl SolanaWebSocketClient {
    pub fn new(
        config: SolanaConfig,
        client: Arc<SolanaClient>,
        event_handler: Arc<dyn EventHandler>,
    ) -> anyhow::Result<Self> {
        let event_parser = EventParser::new(&config.program_id)?;
        let (event_sender, _) = mpsc::unbounded_channel();
        
        Ok(Self {
            config,
            client,
            event_parser,
            event_handler,
            event_sender: Some(event_sender),
            processed_signatures: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
            socket: None,
            reconnect_attempts: Arc::new(tokio::sync::RwLock::new(0)),
            is_connected: Arc::new(tokio::sync::RwLock::new(false)),
        })
    }
    
    pub async fn connect(&mut self) -> anyhow::Result<()> {
        let url = Url::parse(&self.config.ws_url)?;
        
        let config = ClientConfig::new(url);
        
        info!("🔌 Connecting to Solana WebSocket with ezsockets: {}", self.config.ws_url);
        
        let (socket, future) = ezsockets::connect(|_| self.clone(), config).await;
        self.socket = Some(socket);
        
        // Spawn the client future
        tokio::spawn(async move {
            if let Err(e) = future.await {
                error!("WebSocket client error: {}", e);
            }
        });
        
        Ok(())
    }
    
    async fn subscribe_to_logs(&self) -> anyhow::Result<()> {
        if let Some(socket) = &self.socket {
            let subscribe_request = json!({
                "jsonrpc": "2.0",
                "id": Uuid::new_v4().to_string(),
                "method": "logsSubscribe",
                "params": [
                    {
                        "mentions": [self.config.program_id]
                    },
                    {
                        "commitment": self.config.commitment
                    }
                ]
            });
            
            socket.text(subscribe_request.to_string());
            info!("📡 Subscribed to program logs: {}", self.config.program_id);
        }
        
        Ok(())
    }
    
    async fn handle_websocket_message(&self, message: &str) -> anyhow::Result<()> {
        debug!("📨 Received WebSocket message: {}", message);
        
        // Parse JSON message
        let json_msg: Value = serde_json::from_str(message)?;
        
        // Check if it's a subscription confirmation
        if let Some(result) = json_msg.get("result") {
            if json_msg.get("params").is_none() {
                info!("✅ Subscription confirmed: Subscription ID = {}", result);
                return Ok(());
            }
        }
        
        // Handle log notifications
        if let Some(params) = json_msg.get("params") {
            if let Some(result) = params.get("result") {
                let slot = result.get("context")
                    .and_then(|ctx| ctx.get("slot"))
                    .and_then(|s| s.as_u64())
                    .unwrap_or(0);
                
                if let Some(value) = result.get("value") {
                    // Extract signature
                    let signature = match value.get("signature").and_then(|s| s.as_str()) {
                        Some(sig) => sig,
                        None => {
                            warn!("No signature found in message");
                            return Ok(());
                        }
                    };
                    
                    // Check if transaction was successful
                    let transaction_error = value.get("err");
                    let is_transaction_success = transaction_error.is_none() || transaction_error == Some(&Value::Null);
                    
                    if !is_transaction_success {
                        warn!("❌ Transaction {} failed with error: {:?} - SKIPPING processing", signature, transaction_error);
                        return Ok(());
                    }
                    
                    debug!("✅ Transaction {} executed successfully", signature);
                    
                    // Check if already processed
                    {
                        let mut processed = self.processed_signatures.write().await;
                        if processed.contains(signature) {
                            debug!("Signature {} already processed, skipping", signature);
                            return Ok(());
                        }
                        processed.insert(signature.to_string());
                    }
                    
                    // Extract logs
                    if let Some(logs_array) = value.get("logs").and_then(|l| l.as_array()) {
                        let logs: Vec<String> = logs_array
                            .iter()
                            .filter_map(|l| l.as_str())
                            .map(|s| s.to_string())
                            .collect();
                        
                        debug!("📜 Processing {} log entries for signature {}", logs.len(), signature);
                        
                        let mut all_events = Vec::new();
                        
                        // Parse events from logs
                        match self.event_parser.parse_events_with_call_stack(&logs, signature, slot) {
                            Ok(events) => {
                                debug!("Found {} events from logs", events.len());
                                all_events.extend(events);
                            }
                            Err(e) => {
                                debug!("Failed to parse events from logs: {}", e);
                            }
                        }
                        
                        // Handle CPI calls
                        let has_cpi = logs.iter().any(|log| {
                            log.contains("invoke [2]") || 
                            log.contains("invoke [3]") ||
                            log.contains("invoke [4]")
                        });
                        
                        if has_cpi {
                            info!("Detected CPI calls in transaction {}, fetching full details", signature);
                            
                            match self.client.get_transaction_with_logs(signature).await {
                                Ok(tx_details) => {
                                    if !tx_details.is_object() || tx_details.as_object().map_or(true, |o| o.is_empty()) {
                                        debug!("Transaction {} not available yet, using WebSocket logs only", signature);
                                    } else if let Some(meta) = tx_details.get("meta").and_then(|m| m.as_object()) {
                                        if let Some(full_logs) = meta.get("logMessages").and_then(|l| l.as_array()) {
                                            let full_log_strings: Vec<String> = full_logs
                                                .iter()
                                                .filter_map(|l| l.as_str())
                                                .map(|s| s.to_string())
                                                .collect();
                                            
                                            debug!("Got {} logs from transaction details", full_log_strings.len());
                                            
                                            match self.event_parser.parse_events_with_call_stack(&full_log_strings, signature, slot) {
                                                Ok(events) => {
                                                    debug!("Found {} additional events from full transaction", events.len());
                                                    for event in events {
                                                        if !Self::event_exists_in_list(&all_events, &event) {
                                                            all_events.push(event);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    error!("Failed to parse events from full transaction: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to get transaction details for {}: {}", signature, e);
                                }
                            }
                        }
                        
                        // Process all found events
                        if !all_events.is_empty() {
                            info!("✅ Found {} total events in transaction {}", all_events.len(), signature);
                            
                            for event in all_events {
                                if let Err(e) = self.event_handler.handle_event(event).await {
                                    error!("Failed to process event: {}", e);
                                }
                            }
                        } else {
                            debug!("No events found in transaction {}", signature);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn event_exists_in_list(events: &[SpinPetEvent], new_event: &SpinPetEvent) -> bool {
        events.iter().any(|e| {
            Self::events_are_equal(e, new_event)
        })
    }
    
    fn events_are_equal(e1: &SpinPetEvent, e2: &SpinPetEvent) -> bool {
        use SpinPetEvent::*;
        match (e1, e2) {
            (TokenCreated(a), TokenCreated(b)) => a.signature == b.signature,
            (BuySell(a), BuySell(b)) => a.signature == b.signature,
            (LongShort(a), LongShort(b)) => a.signature == b.signature && a.order_pda == b.order_pda,
            (PartialClose(a), PartialClose(b)) => a.signature == b.signature && a.order_pda == b.order_pda,
            (FullClose(a), FullClose(b)) => a.signature == b.signature && a.order_pda == b.order_pda,
            (ForceLiquidate(a), ForceLiquidate(b)) => a.signature == b.signature && a.order_pda == b.order_pda,
            (MilestoneDiscount(a), MilestoneDiscount(b)) => a.signature == b.signature,
            _ => false,
        }
    }
    
    pub async fn get_connection_health(&self) -> serde_json::Value {
        let processed_count = self.processed_signatures.read().await.len();
        let reconnect_attempts = *self.reconnect_attempts.read().await;
        let is_connected = *self.is_connected.read().await;
        
        serde_json::json!({
            "is_connected": is_connected,
            "reconnect_attempts": reconnect_attempts,
            "max_reconnect_attempts": self.config.max_reconnect_attempts,
            "ws_url": self.config.ws_url,
            "program_id": self.config.program_id,
            "processed_signatures_count": processed_count,
            "ping_interval_seconds": self.config.ping_interval_seconds
        })
    }
}

impl Clone for SolanaWebSocketClient {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            client: Arc::clone(&self.client),
            event_parser: self.event_parser.clone(),
            event_handler: Arc::clone(&self.event_handler),
            event_sender: self.event_sender.clone(),
            processed_signatures: Arc::clone(&self.processed_signatures),
            socket: None, // Don't clone the socket
            reconnect_attempts: Arc::clone(&self.reconnect_attempts),
            is_connected: Arc::clone(&self.is_connected),
        }
    }
}

#[async_trait]
impl ezsockets::ClientExt for SolanaWebSocketClient {
    type Params = ();

    async fn text(&mut self, text: String) -> Result<(), Error> {
        if let Err(e) = self.handle_websocket_message(&text).await {
            error!("Failed to handle WebSocket message: {}", e);
        }
        Ok(())
    }

    async fn binary(&mut self, _bytes: Vec<u8>) -> Result<(), Error> {
        debug!("Received binary message (not expected for Solana WebSocket)");
        Ok(())
    }

    async fn call(&mut self, _params: Self::Params) -> Result<(), Error> {
        Ok(())
    }

    async fn on_connect(&mut self) -> Result<(), Error> {
        info!("🔗 WebSocket connected successfully!");
        *self.is_connected.write().await = true;
        *self.reconnect_attempts.write().await = 0;
        
        // Subscribe to logs after connection
        if let Err(e) = self.subscribe_to_logs().await {
            error!("Failed to subscribe to logs: {}", e);
        }
        
        Ok(())
    }

    async fn on_disconnect(&mut self, _frame: Option<CloseFrame>) -> Result<(), Error> {
        warn!("🔌 WebSocket disconnected!");
        *self.is_connected.write().await = false;
        
        let mut attempts = self.reconnect_attempts.write().await;
        *attempts += 1;
        
        if *attempts <= self.config.max_reconnect_attempts {
            info!("🔄 Will attempt to reconnect (attempt {} of {})", *attempts, self.config.max_reconnect_attempts);
        } else {
            error!("❌ Max reconnection attempts ({}) exceeded", self.config.max_reconnect_attempts);
        }
        
        Ok(())
    }

    async fn on_connect_fail(&mut self, _error: Error) -> Result<(), Error> {
        error!("❌ WebSocket connection failed!");
        *self.is_connected.write().await = false;
        
        let mut attempts = self.reconnect_attempts.write().await;
        *attempts += 1;
        
        if *attempts <= self.config.max_reconnect_attempts {
            warn!("🔄 Connection failed, will retry (attempt {} of {})", *attempts, self.config.max_reconnect_attempts);
        } else {
            error!("❌ Max reconnection attempts ({}) exceeded", self.config.max_reconnect_attempts);
        }
        
        Ok(())
    }
}

/// Solana event listener using ezsockets
pub struct SolanaEventListener {
    client: Option<SolanaWebSocketClient>,
    is_running: bool,
}

impl SolanaEventListener {
    pub fn new(
        config: SolanaConfig,
        client: Arc<SolanaClient>,
        event_handler: Arc<dyn EventHandler>,
    ) -> anyhow::Result<Self> {
        let ws_client = SolanaWebSocketClient::new(config, client, event_handler)?;
        
        Ok(Self {
            client: Some(ws_client),
            is_running: false,
        })
    }
    
    pub async fn get_connection_health(&self) -> Option<serde_json::Value> {
        if let Some(client) = &self.client {
            Some(client.get_connection_health().await)
        } else {
            None
        }
    }
}

#[async_trait]
impl EventListener for SolanaEventListener {
    async fn start(&mut self) -> anyhow::Result<()> {
        if self.is_running {
            warn!("Event listener is already running");
            return Ok(());
        }
        
        info!("🚀 Starting Solana event listener with ezsockets");
        
        if let Some(client) = &mut self.client {
            client.connect().await?;
            self.is_running = true;
            info!("✅ Solana event listener started successfully");
        } else {
            return Err(anyhow::anyhow!("WebSocket client not initialized"));
        }
        
        Ok(())
    }
    
    async fn stop(&mut self) -> anyhow::Result<()> {
        if !self.is_running {
            warn!("Event listener is not running");
            return Ok(());
        }
        
        info!("🛑 Stopping Solana event listener");
        
        if let Some(client) = &mut self.client {
            if let Some(socket) = &client.socket {
                socket.close(None);
            }
        }
        
        self.is_running = false;
        info!("✅ Solana event listener stopped successfully");
        
        Ok(())
    }
    
    fn is_running(&self) -> bool {
        self.is_running
    }
}

pub struct EventListenerManager {
    listener: Option<SolanaEventListener>,
}

impl EventListenerManager {
    pub fn new() -> Self {
        Self {
            listener: None,
        }
    }
    
    pub fn initialize(
        &mut self,
        config: SolanaConfig,
        client: Arc<SolanaClient>,
        event_handler: Arc<dyn EventHandler>,
    ) -> anyhow::Result<()> {
        self.listener = Some(SolanaEventListener::new(
            config, 
            client, 
            event_handler
        )?);
        
        Ok(())
    }
    
    pub async fn start(&mut self) -> anyhow::Result<()> {
        if let Some(listener) = &mut self.listener {
            listener.start().await
        } else {
            Err(anyhow::anyhow!("Event listener not initialized"))
        }
    }
    
    #[allow(dead_code)]
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(listener) = &mut self.listener {
            listener.stop().await
        } else {
            Ok(())
        }
    }
    
    pub fn is_running(&self) -> bool {
        self.listener.as_ref().map_or(false, |l| l.is_running())
    }
    
    pub async fn get_connection_health(&self) -> Option<serde_json::Value> {
        if let Some(listener) = &self.listener {
            listener.get_connection_health().await
        } else {
            None
        }
    }
}
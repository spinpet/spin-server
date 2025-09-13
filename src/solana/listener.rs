use super::events::{EventParser, SpinPetEvent};
use super::client::SolanaClient;
use crate::config::SolanaConfig;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{info, error, debug, warn};
use std::sync::Arc;
use std::collections::HashSet;
use async_trait::async_trait;
use uuid::Uuid;

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

/// Solana event listener
pub struct SolanaEventListener {
    config: SolanaConfig,
    client: Arc<SolanaClient>,
    event_parser: EventParser,
    event_handler: Arc<dyn EventHandler>,
    event_sender: Option<mpsc::UnboundedSender<SpinPetEvent>>,
    event_receiver: Option<mpsc::UnboundedReceiver<SpinPetEvent>>,
    reconnect_sender: Option<mpsc::UnboundedSender<()>>,
    reconnect_receiver: Option<mpsc::UnboundedReceiver<()>>,
    is_running: bool,
    reconnect_attempts: u32,
    should_stop: Arc<tokio::sync::RwLock<bool>>,
    processed_signatures: Arc<tokio::sync::RwLock<HashSet<String>>>,
}

impl SolanaEventListener {
    /// Create a new event listener
    pub fn new(
        config: SolanaConfig,
        client: Arc<SolanaClient>,
        event_handler: Arc<dyn EventHandler>,
    ) -> anyhow::Result<Self> {
        let event_parser = EventParser::new(&config.program_id)?;
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let (reconnect_sender, reconnect_receiver) = mpsc::unbounded_channel();
        
        Ok(Self {
            config,
            client,
            event_parser,
            event_handler,
            event_sender: Some(event_sender),
            event_receiver: Some(event_receiver),
            reconnect_sender: Some(reconnect_sender),
            reconnect_receiver: Some(reconnect_receiver),
            is_running: false,
            reconnect_attempts: 0,
            should_stop: Arc::new(tokio::sync::RwLock::new(false)),
            processed_signatures: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
        })
    }

    /// Start event processor
    async fn start_event_processor(&mut self) -> anyhow::Result<()> {
        if let Some(mut receiver) = self.event_receiver.take() {
            let handler = Arc::clone(&self.event_handler);
            
            tokio::spawn(async move {
                info!("🎯 Event processor started");
                
                while let Some(event) = receiver.recv().await {
                    if let Err(e) = handler.handle_event(event).await {
                        error!("Failed to process event: {}", e);
                    }
                }
                
                info!("🎯 Event processor stopped");
            });
        }
        
        Ok(())
    }
    
    /// Start reconnection handler
    async fn start_reconnection_handler(&mut self) -> anyhow::Result<()> {
        if let Some(mut receiver) = self.reconnect_receiver.take() {
            let client = Arc::clone(&self.client);
            let config = self.config.clone();
            let should_stop = Arc::clone(&self.should_stop);
            let event_sender = self.event_sender.clone();
            let event_parser = self.event_parser.clone();
            
            tokio::spawn(async move {
                info!("🔄 Reconnection handler started");
                let mut reconnect_attempts = 0u32;
                
                while let Some(_) = receiver.recv().await {
                    // Check if we should stop
                    if *should_stop.read().await {
                        info!("Reconnection handler received stop signal");
                        break;
                    }
                    
                    info!("🔄 Reconnection signal received, starting reconnection process");
                    
                    // Exponential backoff reconnection loop
                    loop {
                        reconnect_attempts += 1;
                        
                        if reconnect_attempts > config.max_reconnect_attempts {
                            error!("Max reconnection attempts ({}) exceeded. Giving up.", config.max_reconnect_attempts);
                            break;
                        }
                        
                        let delay = config.reconnect_interval * 2u64.pow(reconnect_attempts - 1);
                        let max_delay = 300; // Max 5 minutes
                        let delay = std::cmp::min(delay, max_delay);
                        
                        warn!("🔄 Reconnection attempt {} of {}. Waiting {} seconds before retry...", 
                             reconnect_attempts, config.max_reconnect_attempts, delay);
                             
                        sleep(Duration::from_secs(delay)).await;
                        
                        // Check if we should stop before attempting reconnection
                        if *should_stop.read().await {
                            info!("Stop signal received, aborting reconnection");
                            return;
                        }
                        
                        // Attempt to reconnect  
                        // Note: We don't pass reconnect_sender here to avoid infinite recursion
                        // If this reconnection fails, we'll try again in the next iteration
                        // Create a new processed_signatures set for reconnection
                        let reconnect_processed_sigs = Arc::new(tokio::sync::RwLock::new(HashSet::new()));
                        match Self::connect_websocket_internal(
                            &config,
                            &client,
                            &event_parser,
                            &event_sender,
                            &None, // Don't trigger more reconnect signals from here
                            &should_stop,
                            &reconnect_processed_sigs,
                        ).await {
                            Ok(()) => {
                                info!("✅ Reconnection successful after {} attempts", reconnect_attempts);
                                reconnect_attempts = 0; // Reset counter on successful reconnection
                                break; // Exit the reconnection loop, wait for next signal
                            }
                            Err(e) => {
                                error!("❌ Reconnection attempt {} failed: {}", reconnect_attempts, e);
                                // Continue the loop to try again
                            }
                        }
                    }
                }
                
                info!("🔄 Reconnection handler stopped");
            });
        }
        
        Ok(())
    }

    /// Connect to Solana WebSocket
    async fn connect_websocket(&mut self) -> anyhow::Result<()> {
        Self::connect_websocket_internal(
            &self.config,
            &self.client,
            &self.event_parser,
            &self.event_sender,
            &self.reconnect_sender,
            &self.should_stop,
            &self.processed_signatures,
        ).await
    }
    
    /// Internal WebSocket connection method that can be called statically
    async fn connect_websocket_internal(
        config: &SolanaConfig,
        client: &Arc<SolanaClient>,
        event_parser: &EventParser,
        event_sender: &Option<mpsc::UnboundedSender<SpinPetEvent>>,
        reconnect_sender: &Option<mpsc::UnboundedSender<()>>,
        should_stop: &Arc<tokio::sync::RwLock<bool>>,
        processed_signatures: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    ) -> anyhow::Result<()> {
        let ws_url = &config.ws_url;
        info!("🔌 Connecting to Solana WebSocket: {}", ws_url);

        let (ws_stream, _) = connect_async(ws_url).await?;
        let (mut write, mut read) = ws_stream.split();

        // Subscribe to program logs
        let subscribe_request = json!({
            "jsonrpc": "2.0",
            "id": Uuid::new_v4().to_string(),
            "method": "logsSubscribe",
            "params": [
                {
                    "mentions": [config.program_id]
                },
                {
                    "commitment": "processed" // processed finalized confirmed
                }
            ]
        });

        let subscribe_msg = Message::Text(subscribe_request.to_string());
        write.send(subscribe_msg).await?;
        
        info!("📡 Subscribed to program logs: {}", config.program_id);

        // Handle WebSocket messages
        let event_sender = event_sender.clone();
        let event_parser = event_parser.clone();
        let reconnect_sender = reconnect_sender.clone();
        let should_stop = Arc::clone(should_stop);
        let client = Arc::clone(client);
        let processed_signatures = Arc::clone(processed_signatures);
        
        tokio::spawn(async move {
            info!("🎧 Started listening to WebSocket messages");
            while let Some(msg) = read.next().await {
                // Check if we should stop
                if *should_stop.read().await {
                    info!("WebSocket listener received stop signal");
                    break;
                }
                
                match msg {
                    Ok(Message::Text(text)) => {
                        debug!("📨 Received text message: {}", text);
                        if let Err(e) = Self::handle_websocket_message(
                            &text, 
                            &event_parser, 
                            &event_sender,
                            &client,
                            &processed_signatures
                        ).await {
                            error!("Failed to process WebSocket message: {}", e);
                        }
                    }
                    Ok(Message::Close(_)) => {
                        warn!("WebSocket connection closed, triggering reconnect");
                        // Connection closed - trigger reconnection unless we're stopping
                        if !*should_stop.read().await {
                            if let Some(sender) = &reconnect_sender {
                                if let Err(e) = sender.send(()) {
                                    error!("Failed to send reconnect signal: {}", e);
                                }
                            }
                        }
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        debug!("Received Ping: {:?}", data);
                    }
                    Ok(Message::Pong(data)) => {
                        debug!("Received Pong: {:?}", data);
                    }
                    Err(e) => {
                        error!("WebSocket error: {}, triggering reconnect", e);
                        // WebSocket error - trigger reconnection unless we're stopping
                        if !*should_stop.read().await {
                            if let Some(sender) = &reconnect_sender {
                                if let Err(send_err) = sender.send(()) {
                                    error!("Failed to send reconnect signal: {}", send_err);
                                }
                            }
                        }
                        break;
                    }
                    _ => {
                        debug!("Received other type of message");
                    }
                }
            }
            warn!("🎧 WebSocket message listener ended");
        });

        Ok(())
    }

    /// Handle WebSocket messages with enhanced transaction processing
    async fn handle_websocket_message(
        message: &str,
        event_parser: &EventParser,
        event_sender: &Option<mpsc::UnboundedSender<SpinPetEvent>>,
        client: &Arc<SolanaClient>,
        processed_signatures: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    ) -> anyhow::Result<()> {
        debug!("📨 Received WebSocket message: {}", message);
        
        // 1. 先解析整个JSON消息
        let json_msg: Value = serde_json::from_str(message)?;
        
        // 2. 检查是否是订阅确认消息
        if let Some(result) = json_msg.get("result") {
            if json_msg.get("params").is_none() {
                info!("✅ Subscription confirmed: Subscription ID = {}", result);
                return Ok(());
            }
        }
        
        // 3. 检查是否是日志通知并提取日志
        if let Some(params) = json_msg.get("params") {
            if let Some(result) = params.get("result") {
                let slot = result.get("context")
                    .and_then(|ctx| ctx.get("slot"))
                    .and_then(|s| s.as_u64())
                    .unwrap_or(0);
                
                if let Some(value) = result.get("value") {
                    // 提取签名
                    let signature = match value.get("signature").and_then(|s| s.as_str()) {
                        Some(sig) => sig,
                        None => {
                            warn!("No signature found in message");
                            return Ok(());
                        }
                    };
                    
                    // 检查是否已处理过这个签名
                    {
                        let mut processed = processed_signatures.write().await;
                        if processed.contains(signature) {
                            debug!("Signature {} already processed, skipping", signature);
                            return Ok(());
                        }
                        processed.insert(signature.to_string());
                    }
                    
                    // 提取日志数组
                    if let Some(logs_array) = value.get("logs").and_then(|l| l.as_array()) {
                        let logs: Vec<String> = logs_array
                            .iter()
                            .filter_map(|l| l.as_str())
                            .map(|s| s.to_string())
                            .collect();
                        
                        debug!("📜 Processing {} log entries for signature {}", logs.len(), signature);
                        
                        // 首先尝试从日志中解析事件
                        let mut all_events = Vec::new();
                        
                        // 使用增强的解析方法，支持 CPI 调用栈跟踪
                        match event_parser.parse_events_with_call_stack(&logs, signature, slot) {
                            Ok(events) => {
                                debug!("Found {} events from logs", events.len());
                                all_events.extend(events);
                            }
                            Err(e) => {
                                debug!("Failed to parse events from logs: {}", e);
                            }
                        }
                        
                        // 如果检测到可能有 CPI 调用，获取完整交易详情
                        let has_cpi = logs.iter().any(|log| {
                            log.contains("invoke [2]") || 
                            log.contains("invoke [3]") ||
                            log.contains("invoke [4]")
                        });
                        
                        if has_cpi {
                            info!("Detected CPI calls in transaction {}, fetching full details", signature);
                            
                            // Get full transaction details
                            match client.get_transaction_with_logs(signature).await {
                                Ok(tx_details) => {
                                    // Check if we got valid transaction data
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
                                            
                                            // Re-parse complete logs
                                            match event_parser.parse_events_with_call_stack(&full_log_strings, signature, slot) {
                                                Ok(events) => {
                                                    debug!("Found {} additional events from full transaction", events.len());
                                                    for event in events {
                                                        // Avoid duplicates
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
                        
                        // 发送所有找到的事件
                        if !all_events.is_empty() {
                            info!("✅ Found {} total events in transaction {}", all_events.len(), signature);
                            
                            if let Some(sender) = event_sender {
                                for event in all_events {
                                    debug!("📤 Sending event to processor: {:?}", event);
                                    if let Err(e) = sender.send(event) {
                                        error!("Failed to send event to processor: {}", e);
                                    }
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
    
    /// Check if an event already exists in the list (based on signature and event type)
    fn event_exists_in_list(events: &[SpinPetEvent], new_event: &SpinPetEvent) -> bool {
        events.iter().any(|e| {
            Self::events_are_equal(e, new_event)
        })
    }
    
    /// Compare two events for equality (simplified comparison)
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

}

#[async_trait]
impl EventListener for SolanaEventListener {
    async fn start(&mut self) -> anyhow::Result<()> {
        if self.is_running {
            warn!("Event listener is already running");
            return Ok(());
        }
        
        info!("🚀 Starting Solana event listener");
        
        // Reset stop signal
        *self.should_stop.write().await = false;
        
        // First check if RPC connection works
        if !self.client.check_connection().await? {
            return Err(anyhow::anyhow!("Cannot connect to Solana RPC"));
        }
        
        // Start event processor
        self.start_event_processor().await?;
        
        // Start reconnection handler
        self.start_reconnection_handler().await?;
        
        // Connect to WebSocket
        if let Err(e) = self.connect_websocket().await {
            error!("Failed to initial WebSocket connection: {}", e);
            return Err(e);
        }
        
        self.is_running = true;
        info!("✅ Solana event listener started successfully");
        
        Ok(())
    }
    
    async fn stop(&mut self) -> anyhow::Result<()> {
        if !self.is_running {
            warn!("Event listener is not running");
            return Ok(());
        }
        
        info!("🛑 Stopping Solana event listener");
        
        // Set stop signal to prevent reconnections
        *self.should_stop.write().await = true;
        
        // Close the channels to signal processors to stop
        self.event_sender = None;
        self.reconnect_sender = None;
        
        self.is_running = false;
        info!("✅ Solana event listener stopped successfully");
        
        Ok(())
    }
    
    fn is_running(&self) -> bool {
        self.is_running
    }
}

// Additional methods for SolanaEventListener (not part of EventListener trait)
impl SolanaEventListener {
    /// Get current reconnection attempts count
    pub fn get_reconnect_attempts(&self) -> u32 {
        self.reconnect_attempts
    }
    
    /// Get connection health status
    pub async fn get_connection_health(&self) -> serde_json::Value {
        serde_json::json!({
            "is_running": self.is_running,
            "reconnect_attempts": self.reconnect_attempts,
            "max_reconnect_attempts": self.config.max_reconnect_attempts,
            "should_stop": *self.should_stop.read().await,
            "ws_url": self.config.ws_url,
            "program_id": self.config.program_id
        })
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
            Ok(()) // Not initialized, so no need to stop
        }
    }
    
    pub fn is_running(&self) -> bool {
        self.listener.as_ref().map_or(false, |l| l.is_running())
    }
} 
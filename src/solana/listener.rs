use super::events::{EventParser, SpinPetEvent, TOKEN_CREATED_EVENT_DISCRIMINATOR, BUY_SELL_EVENT_DISCRIMINATOR, LONG_SHORT_EVENT_DISCRIMINATOR, FORCE_LIQUIDATE_EVENT_DISCRIMINATOR, FULL_CLOSE_EVENT_DISCRIMINATOR, PARTIAL_CLOSE_EVENT_DISCRIMINATOR};
use super::client::SolanaClient;
use crate::config::SolanaConfig;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use tracing::{info, error, debug, warn};
use std::sync::Arc;
use async_trait::async_trait;
use uuid::Uuid;
use base64::Engine;

/// Event listener trait
#[async_trait]
pub trait EventListener {
    async fn start(&mut self) -> anyhow::Result<()>;
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
                warn!("⚠️ Force liquidation event: Order {} was liquidated", e.order_pda);
                info!("   - Liquidator: {}", e.payer);
                info!("   - Token: {}", e.mint_account);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::FullClose(e) => {
                let direction = if e.is_close_long { "closed long" } else { "closed short" };
                info!("🔒 Full close event: {} {} on token {} (order PDA: {})", 
                      e.payer, direction, e.mint_account, e.order_pda);
                info!("   - User SOL account: {}", e.user_sol_account);
                info!("   - Final token amount: {}", e.final_token_amount);
                info!("   - Final SOL amount: {}", e.final_sol_amount);
                info!("   - User profit: {}", e.user_close_profit);
                info!("   - Latest price: {}", e.latest_price);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::PartialClose(e) => {
                let direction = if e.is_close_long { "partially closed long" } else { "partially closed short" };
                info!("🔓 Partial close event: {} {} on token {} (order PDA: {})", 
                      e.payer, direction, e.mint_account, e.order_pda);
                info!("   - User SOL account: {}", e.user_sol_account);
                info!("   - Final token amount: {}", e.final_token_amount);
                info!("   - Final SOL amount: {}", e.final_sol_amount);
                info!("   - User profit: {}", e.user_close_profit);
                info!("   - Latest price: {}", e.latest_price);
                info!("   - Remaining position: {}", e.position_asset_amount);
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
    is_running: bool,
    reconnect_attempts: u32,
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
        
        Ok(Self {
            config,
            client,
            event_parser,
            event_handler,
            event_sender: Some(event_sender),
            event_receiver: Some(event_receiver),
            is_running: false,
            reconnect_attempts: 0,
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

    /// Connect to Solana WebSocket
    async fn connect_websocket(&mut self) -> anyhow::Result<()> {
        let ws_url = &self.config.ws_url;
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
                    "mentions": [self.config.program_id]
                },
                {
                    "commitment": "confirmed"
                }
            ]
        });

        let subscribe_msg = Message::Text(subscribe_request.to_string());
        write.send(subscribe_msg).await?;
        
        info!("📡 Subscribed to program logs: {}", self.config.program_id);

        // Handle WebSocket messages
        let event_sender = self.event_sender.clone();
        let event_parser = self.event_parser.clone();
        
        tokio::spawn(async move {
            info!("🎧 Started listening to WebSocket messages");
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        debug!("📨 Received text message: {}", text);
                        if let Err(e) = Self::handle_websocket_message(
                            &text, 
                            &event_parser, 
                            &event_sender
                        ).await {
                            error!("Failed to process WebSocket message: {}", e);
                        }
                    }
                    Ok(Message::Close(_)) => {
                        warn!("WebSocket connection closed");
                        break;
                    }
                    Ok(Message::Ping(data)) => {
                        debug!("Received Ping: {:?}", data);
                    }
                    Ok(Message::Pong(data)) => {
                        debug!("Received Pong: {:?}", data);
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {
                        debug!("Received other type of message");
                    }
                }
            }
            warn!("🎧 WebSocket message listener ended");
        });

        self.reconnect_attempts = 0;
        Ok(())
    }

    /// Handle WebSocket messages
    async fn handle_websocket_message(
        message: &str,
        event_parser: &EventParser,
        event_sender: &Option<mpsc::UnboundedSender<SpinPetEvent>>,
    ) -> anyhow::Result<()> {
        debug!("📨 Received WebSocket message: {}", message);
        let json_msg: Value = serde_json::from_str(message)?;
        
        // Check if this is a subscription confirmation message
        if let Some(result) = json_msg.get("result") {
            if json_msg.get("params").is_none() {
                info!("✅ Subscription confirmed: Subscription ID = {}", result);
                return Ok(());
            }
        }
        
        // Check if this is a log notification
        if let Some(params) = json_msg.get("params") {
            if let Some(result) = params.get("result") {
                if let Some(value) = result.get("value") {
                    if let Some(signature) = value.get("signature").and_then(|s| s.as_str()) {
                        if let Some(slot) = value.get("slot").and_then(|s| s.as_u64()) {
                            if let Some(logs) = value.get("logs").and_then(|l| l.as_array()) {
                                let logs: Vec<String> = logs
                                    .iter()
                                    .filter_map(|l| l.as_str())
                                    .map(|s| s.to_string())
                                    .collect();
                                
                                debug!("📜 Processing transaction logs, signature: {}, slot: {}", signature, slot);
                                
                                // Parse events from logs
                                match event_parser.parse_event_from_logs(&logs, signature, slot) {
                                    Ok(events) => {
                                        if events.is_empty() {
                                            debug!("No events found in logs");
                                        } else {
                                            debug!("Found {} events in logs", events.len());
                                            
                                            if let Some(sender) = event_sender {
                                                for event in events {
                                                    debug!("📤 Sending event to processor");
                                                    if let Err(e) = sender.send(event) {
                                                        error!("Failed to send event to processor: {}", e);
                                                    }
                                                }
                                            } else {
                                                warn!("No event sender available");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to parse events from logs: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Reconnect to WebSocket with exponential backoff
    async fn reconnect(&mut self) -> anyhow::Result<()> {
        self.reconnect_attempts += 1;
        
        if self.reconnect_attempts > self.config.max_reconnect_attempts {
            error!("Max reconnection attempts ({}) exceeded. Giving up.", self.config.max_reconnect_attempts);
            return Err(anyhow::anyhow!("Max reconnection attempts exceeded"));
        }
        
        let delay = self.config.reconnect_interval * 2u64.pow(self.reconnect_attempts - 1);
        let max_delay = 300; // Max 5 minutes
        let delay = std::cmp::min(delay, max_delay);
        
        warn!("Reconnection attempt {} of {}. Waiting {} seconds before retry...", 
             self.reconnect_attempts, self.config.max_reconnect_attempts, delay);
             
        sleep(Duration::from_secs(delay)).await;
        self.connect_websocket().await
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
        
        // First check if RPC connection works
        if !self.client.check_connection().await? {
            return Err(anyhow::anyhow!("Cannot connect to Solana RPC"));
        }
        
        // Start event processor
        self.start_event_processor().await?;
        
        // Connect to WebSocket
        if let Err(e) = self.connect_websocket().await {
            error!("Failed to connect to WebSocket: {}", e);
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
        
        // Close the event sender to signal processor to stop
        self.event_sender = None;
        
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
use super::client::SolanaClient;
use super::events::{EventParser, SpinPetEvent};
use crate::config::SolanaConfig;
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use rand;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};
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

    /// Downcast support for trait objects
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Default event handler - simply print events
pub struct DefaultEventHandler;

#[async_trait]
impl EventHandler for DefaultEventHandler {
    async fn handle_event(&self, event: SpinPetEvent) -> anyhow::Result<()> {
        match event {
            SpinPetEvent::TokenCreated(e) => {
                info!(
                    "🪙 Token creation event: {} created token {}",
                    e.payer, e.mint_account
                );
                info!("   - Token name: {}", e.name);
                info!("   - Token symbol: {}", e.symbol);
                info!("   - Curve account: {}", e.curve_account);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::BuySell(e) => {
                let action = if e.is_buy { "bought" } else { "sold" };
                info!(
                    "💰 Trade event: {} {} token {} (token amount: {}, SOL amount: {})",
                    e.payer, action, e.mint_account, e.token_amount, e.sol_amount
                );
                info!("   - Latest price: {}", e.latest_price);
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::LongShort(e) => {
                let direction = if e.order_type == 1 { "long" } else { "short" };
                info!(
                    "📈 Long/Short event: {} went {} on token {} (order PDA: {})",
                    e.payer, direction, e.mint_account, e.order_pda
                );
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
                info!(
                    "⚠️ Force liquidation event: {} liquidated order {} on token {}",
                    e.payer, e.order_pda, e.mint_account
                );
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
            SpinPetEvent::FullClose(e) => {
                let direction = if e.is_close_long { "long" } else { "short" };
                info!(
                    "🔒 Full close event: {} closed {} order {} on token {} (profit: {})",
                    e.payer, direction, e.order_pda, e.mint_account, e.user_close_profit
                );
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
                info!(
                    "💲 Milestone discount event: {} updated fees for token {}",
                    e.payer, e.mint_account
                );
                info!("   - Swap fee: {}", e.swap_fee);
                info!("   - Borrow fee: {}", e.borrow_fee);
                info!(
                    "   - Fee discount flag: {} (0: 原价, 1: 5折, 2: 2.5折, 3: 1.25折)",
                    e.fee_discount_flag
                );
                info!("   - Transaction signature: {}", e.signature);
                info!("   - Block height: {}", e.slot);
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Improved Solana event listener with robust reconnection
pub struct SolanaEventListener {
    config: SolanaConfig,
    client: Arc<SolanaClient>,
    event_parser: EventParser,
    event_handler: Arc<dyn EventHandler>,
    // Use broadcast channel to avoid "channel closed" errors
    event_broadcaster: broadcast::Sender<SpinPetEvent>,
    connection_state: Arc<tokio::sync::RwLock<ConnectionState>>,
    reconnect_attempts: Arc<tokio::sync::RwLock<u32>>,
    should_stop: Arc<tokio::sync::RwLock<bool>>,
    processed_signatures: Arc<tokio::sync::RwLock<HashSet<String>>>,
    is_running: bool,
}

impl SolanaEventListener {
    /// Create a new event listener
    pub fn new(
        config: SolanaConfig,
        client: Arc<SolanaClient>,
        event_handler: Arc<dyn EventHandler>,
    ) -> anyhow::Result<Self> {
        let event_parser = EventParser::new(&config.program_id)?;
        let (event_broadcaster, _) = broadcast::channel(1000);

        Ok(Self {
            config,
            client,
            event_parser,
            event_handler,
            event_broadcaster,
            connection_state: Arc::new(tokio::sync::RwLock::new(ConnectionState::Disconnected)),
            reconnect_attempts: Arc::new(tokio::sync::RwLock::new(0)),
            should_stop: Arc::new(tokio::sync::RwLock::new(false)),
            processed_signatures: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
            is_running: false,
        })
    }

    /// Start event processor using broadcast channel
    async fn start_event_processor(&self) -> anyhow::Result<()> {
        let mut event_receiver = self.event_broadcaster.subscribe();
        let handler = Arc::clone(&self.event_handler);
        let should_stop = Arc::clone(&self.should_stop);

        tokio::spawn(async move {
            info!("🎯 Event processor started with broadcast channel");

            loop {
                tokio::select! {
                    event_result = event_receiver.recv() => {
                        match event_result {
                            Ok(event) => {
                                if let Err(e) = handler.handle_event(event).await {
                                    error!("Failed to process event: {}", e);
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                                warn!("Event processor lagged, skipped {} events", skipped);
                                continue;
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                info!("Event broadcaster closed, stopping processor");
                                break;
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(100)) => {
                        if *should_stop.read().await {
                            info!("Event processor received stop signal");
                            break;
                        }
                    }
                }
            }

            info!("🎯 Event processor stopped");
        });

        Ok(())
    }

    /// Main connection loop with automatic reconnection
    async fn connection_loop(&self) -> anyhow::Result<()> {
        let config = self.config.clone();
        let client = Arc::clone(&self.client);
        let event_parser = self.event_parser.clone();
        let event_broadcaster = self.event_broadcaster.clone();
        let connection_state = Arc::clone(&self.connection_state);
        let reconnect_attempts = Arc::clone(&self.reconnect_attempts);
        let should_stop = Arc::clone(&self.should_stop);
        let processed_signatures = Arc::clone(&self.processed_signatures);

        tokio::spawn(async move {
            info!("🔄 Starting connection loop");

            loop {
                // Check if we should stop
                if *should_stop.read().await {
                    info!("Connection loop received stop signal");
                    break;
                }

                *connection_state.write().await = ConnectionState::Connecting;
                info!("🔌 Attempting to connect to WebSocket: {}", config.ws_url);

                match Self::connect_and_listen(
                    &config,
                    &client,
                    &event_parser,
                    &event_broadcaster,
                    &connection_state,
                    &should_stop,
                    &processed_signatures,
                )
                .await
                {
                    Ok(()) => {
                        info!("✅ WebSocket connection completed normally");
                        *reconnect_attempts.write().await = 0;
                    }
                    Err(e) => {
                        error!("❌ WebSocket connection failed: {}", e);
                        let mut attempts = reconnect_attempts.write().await;
                        *attempts += 1;

                        if *attempts > config.max_reconnect_attempts {
                            error!(
                                "❌ Max reconnection attempts ({}) exceeded",
                                config.max_reconnect_attempts
                            );
                            *connection_state.write().await = ConnectionState::Disconnected;
                            break;
                        }

                        *connection_state.write().await = ConnectionState::Reconnecting;

                        // Exponential backoff with jitter
                        let base_delay = config.reconnect_interval;
                        let exponential_delay =
                            std::cmp::min(base_delay * 2_u64.pow((*attempts - 1).min(5)), 60);
                        let jitter = (rand::random::<f64>() * 2.0) as u64;
                        let delay = exponential_delay + jitter;

                        warn!(
                            "🔄 Reconnection attempt {} of {} in {} seconds",
                            *attempts, config.max_reconnect_attempts, delay
                        );

                        drop(attempts);
                        sleep(Duration::from_secs(delay)).await;
                    }
                }
            }

            *connection_state.write().await = ConnectionState::Disconnected;
            info!("🔄 Connection loop ended");
        });

        Ok(())
    }

    /// Connect and listen to WebSocket
    async fn connect_and_listen(
        config: &SolanaConfig,
        client: &Arc<SolanaClient>,
        event_parser: &EventParser,
        event_broadcaster: &broadcast::Sender<SpinPetEvent>,
        connection_state: &Arc<tokio::sync::RwLock<ConnectionState>>,
        should_stop: &Arc<tokio::sync::RwLock<bool>>,
        processed_signatures: &Arc<tokio::sync::RwLock<HashSet<String>>>,
    ) -> anyhow::Result<()> {
        let (ws_stream, _) = connect_async(&config.ws_url).await?;
        info!("🔗 WebSocket connected successfully");

        *connection_state.write().await = ConnectionState::Connected;

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
                    "commitment": config.commitment
                }
            ]
        });

        let subscribe_msg = Message::Text(subscribe_request.to_string());
        write.send(subscribe_msg).await?;
        info!("📡 Subscribed to program logs: {}", config.program_id);

        // Shared writer for ping and other operations
        let shared_writer = Arc::new(Mutex::new(write));
        let (ping_stop_sender, mut ping_stop_receiver) = mpsc::unbounded_channel::<()>();

        // Start ping task
        let ping_writer = Arc::clone(&shared_writer);
        let ping_should_stop = Arc::clone(should_stop);
        let ping_config = config.clone();
        tokio::spawn(async move {
            info!(
                "💓 Starting ping task (every {} seconds)",
                ping_config.ping_interval_seconds
            );
            let mut ping_interval =
                interval(Duration::from_secs(ping_config.ping_interval_seconds));
            ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut consecutive_failures = 0u32;
            const MAX_PING_FAILURES: u32 = 3;

            loop {
                tokio::select! {
                    _ = ping_interval.tick() => {
                        if *ping_should_stop.read().await {
                            break;
                        }

                        let mut writer = ping_writer.lock().await;
                        match writer.send(Message::Ping(vec![])).await {
                            Ok(()) => {
                                consecutive_failures = 0;
                                debug!("💓 Ping sent successfully");
                            }
                            Err(e) => {
                                consecutive_failures += 1;
                                warn!("💓 Ping failed ({}): {}", consecutive_failures, e);

                                if consecutive_failures >= MAX_PING_FAILURES {
                                    error!("💓 Too many ping failures, connection seems dead");
                                    break;
                                }
                            }
                        }
                    }
                    _ = ping_stop_receiver.recv() => {
                        info!("💓 Ping task received stop signal");
                        break;
                    }
                }
            }
            info!("💓 Ping task stopped");
        });

        // Message handling loop
        let event_broadcaster_clone = event_broadcaster.clone();
        let event_parser_clone = event_parser.clone();
        let client_clone = Arc::clone(client);
        let processed_signatures_clone = Arc::clone(processed_signatures);
        let should_stop_clone = Arc::clone(should_stop);

        info!("🎧 Starting to listen for WebSocket messages");
        while let Some(msg) = read.next().await {
            // Check stop signal
            if *should_stop_clone.read().await {
                info!("Message listener received stop signal");
                break;
            }

            match msg {
                Ok(Message::Text(text)) => {
                    debug!("📨 Received text message");
                    if let Err(e) = Self::handle_websocket_message(
                        &text,
                        &event_parser_clone,
                        &event_broadcaster_clone,
                        &client_clone,
                        &processed_signatures_clone,
                        config,
                    )
                    .await
                    {
                        error!("Failed to process WebSocket message: {}", e);
                    }
                }
                Ok(Message::Close(_)) => {
                    warn!("🎧 WebSocket connection closed by server");
                    break;
                }
                Ok(Message::Ping(data)) => {
                    debug!("🏓 Received ping, responding with pong");
                    let mut writer = shared_writer.lock().await;
                    if let Err(e) = writer.send(Message::Pong(data)).await {
                        warn!("Failed to send pong: {}", e);
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {
                    debug!("🏓 Received pong - connection alive");
                }
                Err(e) => {
                    error!("🎧 WebSocket error: {}", e);
                    break;
                }
                _ => {
                    debug!("Received other message type");
                }
            }
        }

        // Stop ping task
        let _ = ping_stop_sender.send(());
        warn!("🎧 WebSocket message listener ended");

        Ok(())
    }

    /// Handle WebSocket messages (same logic as before)
    async fn handle_websocket_message(
        message: &str,
        event_parser: &EventParser,
        event_broadcaster: &broadcast::Sender<SpinPetEvent>,
        client: &Arc<SolanaClient>,
        processed_signatures: &Arc<tokio::sync::RwLock<HashSet<String>>>,
        config: &SolanaConfig,
    ) -> anyhow::Result<()> {
        debug!("📨 Processing WebSocket message");

        let json_msg: Value = serde_json::from_str(message)?;

        // Check subscription confirmation
        if let Some(result) = json_msg.get("result") {
            if json_msg.get("params").is_none() {
                info!("✅ Subscription confirmed: ID = {}", result);
                return Ok(());
            }
        }

        // Handle log notifications
        if let Some(params) = json_msg.get("params") {
            if let Some(result) = params.get("result") {
                let slot = result
                    .get("context")
                    .and_then(|ctx| ctx.get("slot"))
                    .and_then(|s| s.as_u64())
                    .unwrap_or(0);

                if let Some(value) = result.get("value") {
                    let signature = match value.get("signature").and_then(|s| s.as_str()) {
                        Some(sig) => sig,
                        None => {
                            warn!("No signature found in message");
                            return Ok(());
                        }
                    };

                    // Check transaction success
                    let transaction_error = value.get("err");
                    let is_transaction_success =
                        transaction_error.is_none() || transaction_error == Some(&Value::Null);

                    if !is_transaction_success {
                        if let Some(error_detail) = transaction_error {
                            debug!(
                                "❌ Transaction {} failed with error: {}",
                                signature, error_detail
                            );
                        } else {
                            debug!("❌ Transaction {} failed with unknown error", signature);
                        }

                        // Skip failed transactions unless explicitly configured to process them
                        if !config.process_failed_transactions {
                            debug!("⏭️ Skipping failed transaction {} (process_failed_transactions=false)", signature);
                            return Ok(());
                        } else {
                            debug!("🔄 Processing failed transaction {} (process_failed_transactions=true)", signature);
                        }
                    }

                    // Check if already processed
                    {
                        let mut processed = processed_signatures.write().await;
                        if processed.contains(signature) {
                            debug!("Signature {} already processed", signature);
                            return Ok(());
                        }
                        processed.insert(signature.to_string());
                    }

                    // Process logs
                    if let Some(logs_array) = value.get("logs").and_then(|l| l.as_array()) {
                        let logs: Vec<String> = logs_array
                            .iter()
                            .filter_map(|l| l.as_str())
                            .map(|s| s.to_string())
                            .collect();

                        let mut all_events = Vec::new();

                        // Parse events from logs
                        match event_parser.parse_events_with_call_stack(&logs, signature, slot) {
                            Ok(events) => {
                                all_events.extend(events);
                            }
                            Err(e) => {
                                debug!("Failed to parse events from logs: {}", e);
                            }
                        }

                        // Handle CPI calls if needed
                        let has_cpi = logs.iter().any(|log| {
                            log.contains("invoke [2]")
                                || log.contains("invoke [3]")
                                || log.contains("invoke [4]")
                        });

                        if has_cpi {
                            info!("Detected CPI calls, fetching full transaction details");

                            match client.get_transaction_with_logs(signature).await {
                                Ok(tx_details) => {
                                    if let Some(meta) =
                                        tx_details.get("meta").and_then(|m| m.as_object())
                                    {
                                        if let Some(full_logs) =
                                            meta.get("logMessages").and_then(|l| l.as_array())
                                        {
                                            let full_log_strings: Vec<String> = full_logs
                                                .iter()
                                                .filter_map(|l| l.as_str())
                                                .map(|s| s.to_string())
                                                .collect();

                                            match event_parser.parse_events_with_call_stack(
                                                &full_log_strings,
                                                signature,
                                                slot,
                                            ) {
                                                Ok(events) => {
                                                    for event in events {
                                                        if !Self::event_exists_in_list(
                                                            &all_events,
                                                            &event,
                                                        ) {
                                                            all_events.push(event);
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    error!("Failed to parse full transaction events: {}", e);
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to get transaction details: {}", e);
                                }
                            }
                        }

                        // Broadcast events
                        if !all_events.is_empty() {
                            info!(
                                "✅ Broadcasting {} events for transaction {}",
                                all_events.len(),
                                signature
                            );

                            for event in all_events {
                                if let Err(e) = event_broadcaster.send(event) {
                                    error!("Failed to broadcast event: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn event_exists_in_list(events: &[SpinPetEvent], new_event: &SpinPetEvent) -> bool {
        events.iter().any(|e| Self::events_are_equal(e, new_event))
    }

    fn events_are_equal(e1: &SpinPetEvent, e2: &SpinPetEvent) -> bool {
        use SpinPetEvent::*;
        match (e1, e2) {
            (TokenCreated(a), TokenCreated(b)) => a.signature == b.signature,
            (BuySell(a), BuySell(b)) => a.signature == b.signature,
            (LongShort(a), LongShort(b)) => {
                a.signature == b.signature && a.order_pda == b.order_pda
            }
            (PartialClose(a), PartialClose(b)) => {
                a.signature == b.signature && a.order_pda == b.order_pda
            }
            (FullClose(a), FullClose(b)) => {
                a.signature == b.signature && a.order_pda == b.order_pda
            }
            (ForceLiquidate(a), ForceLiquidate(b)) => {
                a.signature == b.signature && a.order_pda == b.order_pda
            }
            (MilestoneDiscount(a), MilestoneDiscount(b)) => a.signature == b.signature,
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub async fn get_connection_health(&self) -> serde_json::Value {
        let processed_count = self.processed_signatures.read().await.len();
        let current_attempts = *self.reconnect_attempts.read().await;
        let connection_state = self.connection_state.read().await.clone();

        serde_json::json!({
            "is_running": self.is_running,
            "connection_state": format!("{:?}", connection_state),
            "reconnect_attempts": current_attempts,
            "max_reconnect_attempts": self.config.max_reconnect_attempts,
            "should_stop": *self.should_stop.read().await,
            "ws_url": self.config.ws_url,
            "program_id": self.config.program_id,
            "processed_signatures_count": processed_count,
            "ping_interval_seconds": self.config.ping_interval_seconds
        })
    }
}

#[async_trait]
impl EventListener for SolanaEventListener {
    async fn start(&mut self) -> anyhow::Result<()> {
        if self.is_running {
            warn!("Event listener is already running");
            return Ok(());
        }

        info!("🚀 Starting improved Solana event listener");

        // Reset stop signal
        *self.should_stop.write().await = false;

        // Check RPC connection
        if !self.client.check_connection().await? {
            return Err(anyhow::anyhow!("Cannot connect to Solana RPC"));
        }

        // Start event processor
        self.start_event_processor().await?;

        // Start connection loop
        self.connection_loop().await?;

        self.is_running = true;
        info!("✅ Improved Solana event listener started successfully");

        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        if !self.is_running {
            warn!("Event listener is not running");
            return Ok(());
        }

        info!("🛑 Stopping improved Solana event listener");

        // Set stop signal
        *self.should_stop.write().await = true;

        // Allow some time for graceful shutdown
        sleep(Duration::from_secs(2)).await;

        self.is_running = false;
        info!("✅ Improved Solana event listener stopped successfully");

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
        Self { listener: None }
    }

    pub fn initialize(
        &mut self,
        config: SolanaConfig,
        client: Arc<SolanaClient>,
        event_handler: Arc<dyn EventHandler>,
    ) -> anyhow::Result<()> {
        self.listener = Some(SolanaEventListener::new(config, client, event_handler)?);

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

    #[allow(dead_code)]
    pub async fn get_connection_health(&self) -> Option<serde_json::Value> {
        if let Some(listener) = &self.listener {
            Some(listener.get_connection_health().await)
        } else {
            None
        }
    }
}

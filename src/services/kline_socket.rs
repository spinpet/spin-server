// K线实时推送 Socket.IO 服务
// 基于 SocketIoxide 0.17 实现

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use socketioxide::extract::{Data, SocketRef};
use socketioxide::SocketIo;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use utoipa::ToSchema;

use crate::models::{KlineData, KlineQuery};
use crate::services::event_service::StatsEventHandler;
use crate::services::event_storage::EventStorage;
use crate::solana::events::SpinPetEvent;
use crate::solana::EventHandler;

/// K线推送服务配置
#[derive(Debug, Clone)]
pub struct KlineConfig {
    pub connection_timeout: Duration,        // 连接超时时间 (默认60秒)
    pub max_subscriptions_per_client: usize, // 每客户端最大订阅数 (默认100)
    #[allow(dead_code)]
    pub history_data_limit: usize, // 历史数据默认条数 (默认100)
    pub ping_interval: Duration,             // 心跳间隔 (默认25秒)
    pub ping_timeout: Duration,              // 心跳超时 (默认60秒)
}

impl Default for KlineConfig {
    fn default() -> Self {
        Self {
            connection_timeout: Duration::from_secs(60),
            max_subscriptions_per_client: 100,
            history_data_limit: 100,
            ping_interval: Duration::from_secs(25),
            ping_timeout: Duration::from_secs(60),
        }
    }
}

impl KlineConfig {
    pub fn from_config(config: &crate::config::KlineServiceConfig) -> Self {
        Self {
            connection_timeout: Duration::from_secs(config.connection_timeout_secs),
            max_subscriptions_per_client: config.max_subscriptions_per_client,
            history_data_limit: config.history_data_limit,
            ping_interval: Duration::from_secs(config.ping_interval_secs),
            ping_timeout: Duration::from_secs(config.ping_timeout_secs),
        }
    }
}

/// 客户端连接信息
#[derive(Debug, Clone)]
pub struct ClientConnection {
    #[allow(dead_code)]
    pub socket_id: String,
    pub subscriptions: HashSet<String>, // "mint:interval" 格式
    pub last_activity: Instant,         // 最后活动时间
    pub connection_time: Instant,       // 连接建立时间
    pub subscription_count: usize,      // 当前订阅数量
    #[allow(dead_code)]
    pub user_agent: Option<String>, // 客户端信息
    pub kline_data_sent_count: u64,     // kline_data 发送次数
    pub history_data_sent_count: u64,   // history_data 发送次数
    pub total_messages_sent: u64,       // 总消息发送次数
}

/// 订阅管理器
#[derive(Debug)]
pub struct SubscriptionManager {
    // 连接映射: SocketId -> 客户端信息
    pub connections: HashMap<String, ClientConnection>,

    // 订阅索引: mint_account -> interval -> SocketId集合
    pub mint_subscribers: HashMap<String, HashMap<String, HashSet<String>>>,

    // 反向索引: SocketId -> 订阅键集合 (用于快速清理)
    pub client_subscriptions: HashMap<String, HashSet<String>>,
}

impl SubscriptionManager {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            mint_subscribers: HashMap::new(),
            client_subscriptions: HashMap::new(),
        }
    }

    pub fn add_subscription(&mut self, socket_id: &str, mint: &str, interval: &str) -> Result<()> {
        // 检查客户端是否存在
        let client = self
            .connections
            .get_mut(socket_id)
            .ok_or_else(|| anyhow::anyhow!("Client not found"))?;

        // 检查订阅数量限制
        if client.subscription_count >= 100 {
            // 可配置
            return Err(anyhow::anyhow!("Subscription limit exceeded"));
        }

        let subscription_key = format!("{}:{}", mint, interval);

        // 添加到客户端订阅列表
        if client.subscriptions.insert(subscription_key.clone()) {
            client.subscription_count += 1;

            // 添加到全局索引
            self.mint_subscribers
                .entry(mint.to_string())
                .or_default()
                .entry(interval.to_string())
                .or_default()
                .insert(socket_id.to_string());

            // 添加到反向索引
            self.client_subscriptions
                .entry(socket_id.to_string())
                .or_default()
                .insert(subscription_key);
        }

        Ok(())
    }

    pub fn remove_subscription(&mut self, socket_id: &str, mint: &str, interval: &str) {
        let subscription_key = format!("{}:{}", mint, interval);

        // 从客户端订阅列表移除
        if let Some(client) = self.connections.get_mut(socket_id) {
            if client.subscriptions.remove(&subscription_key) {
                client.subscription_count = client.subscription_count.saturating_sub(1);
            }
        }

        // 从全局索引移除
        if let Some(interval_map) = self.mint_subscribers.get_mut(mint) {
            if let Some(client_set) = interval_map.get_mut(interval) {
                client_set.remove(socket_id);

                if client_set.is_empty() {
                    interval_map.remove(interval);
                }
            }

            if interval_map.is_empty() {
                self.mint_subscribers.remove(mint);
            }
        }

        // 从反向索引移除
        if let Some(subscriptions) = self.client_subscriptions.get_mut(socket_id) {
            subscriptions.remove(&subscription_key);
        }
    }

    pub fn get_subscribers(&self, mint: &str, interval: &str) -> Vec<String> {
        self.mint_subscribers
            .get(mint)
            .and_then(|interval_map| interval_map.get(interval))
            .map(|client_set| client_set.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn remove_client(&mut self, socket_id: &str) {
        // 获取该客户端的所有订阅
        if let Some(subscriptions) = self.client_subscriptions.remove(socket_id) {
            for subscription_key in subscriptions {
                let parts: Vec<&str> = subscription_key.split(':').collect();
                if parts.len() == 2 {
                    let (mint, interval) = (parts[0], parts[1]);
                    self.remove_subscription(socket_id, mint, interval);
                }
            }
        }

        // 移除连接记录
        self.connections.remove(socket_id);
    }

    pub fn update_activity(&mut self, socket_id: &str) {
        if let Some(client) = self.connections.get_mut(socket_id) {
            client.last_activity = Instant::now();
        }
    }
}

/// 实时K线推送消息
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct KlineUpdateMessage {
    pub symbol: String,                  // mint_account
    pub interval: String,                // s1, s30, m5
    pub subscription_id: Option<String>, // 客户端订阅ID
    pub data: KlineRealtimeData,         // K线数据
    pub timestamp: u64,                  // 推送时间戳（毫秒）
}

/// 实时K线数据结构（基于现有KlineData扩展）
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct KlineRealtimeData {
    pub time: u64,           // Unix时间戳（秒）
    pub open: f64,           // 开盘价
    pub high: f64,           // 最高价
    pub low: f64,            // 最低价
    pub close: f64,          // 收盘价（当前价格）
    pub volume: f64,         // 成交量（项目要求为0）
    pub is_final: bool,      // 是否为最终K线
    pub update_type: String, // "realtime" | "final"
    pub update_count: u32,   // 更新次数
}

/// 历史数据响应
#[derive(Debug, Serialize, ToSchema)]
pub struct KlineHistoryResponse {
    pub symbol: String,
    pub interval: String,
    pub data: Vec<KlineRealtimeData>,
    pub has_more: bool,
    pub total_count: usize,
}

/// 交易事件推送消息
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct EventUpdateMessage {
    pub symbol: String,                  // mint_account
    pub event_type: String,              // event type name
    pub event_data: SpinPetEvent,        // complete event data
    pub timestamp: u64,                  // push timestamp (milliseconds)
}

/// 历史交易事件响应
#[derive(Debug, Serialize, ToSchema)]
pub struct EventHistoryResponse {
    pub symbol: String,
    pub data: Vec<EventUpdateMessage>,
    pub has_more: bool,
    pub total_count: usize,
}

/// Socket.IO 请求消息
#[derive(Debug, Deserialize)]
pub struct SubscribeRequest {
    pub symbol: String,                  // mint_account
    pub interval: String,                // s1, s30, m5
    pub subscription_id: Option<String>, // 客户端订阅ID
}

#[derive(Debug, Deserialize)]
pub struct UnsubscribeRequest {
    pub symbol: String,
    pub interval: String,
    pub subscription_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryRequest {
    pub symbol: String,
    pub interval: String,
    pub limit: Option<usize>,
    #[allow(dead_code)]
    pub from: Option<u64>, // 开始时间戳（秒）
}

/// K线推送服务
pub struct KlineSocketService {
    pub socketio: SocketIo,                              // SocketIoxide 实例
    pub event_storage: Arc<EventStorage>,                // 现有事件存储
    pub subscriptions: Arc<RwLock<SubscriptionManager>>, // 订阅管理
    pub config: KlineConfig,                             // 配置参数
}

impl KlineSocketService {
    pub fn new(
        event_storage: Arc<EventStorage>,
        config: KlineConfig,
    ) -> Result<(Self, socketioxide::layer::SocketIoLayer)> {
        // 创建 SocketIoxide 实例
        let (layer, io) = SocketIo::builder()
            .ping_interval(config.ping_interval)
            .ping_timeout(config.ping_timeout)
            .max_payload(1024 * 1024) // 1MB 最大负载
            .build_layer();

        let service = Self {
            socketio: io,
            event_storage,
            subscriptions: Arc::new(RwLock::new(SubscriptionManager::new())),
            config,
        };

        Ok((service, layer))
    }

    /// 设置事件处理器
    pub fn setup_socket_handlers(&self) {
        let subscriptions = Arc::clone(&self.subscriptions);
        let event_storage = Arc::clone(&self.event_storage);

        // 设置默认命名空间（避免default namespace not found错误）
        self.socketio.ns("/", |_socket: SocketRef| {
            // 默认命名空间不做任何处理，只是为了避免错误
        });

        // K线命名空间 - 合并所有事件处理器到一个命名空间
        self.socketio.ns("/kline", {
            let subscriptions = subscriptions.clone();
            let event_storage = event_storage.clone();

            move |socket: SocketRef| {
                info!("🔌 New client connected to /kline: {}", socket.id);

                // 保存 socket_id 用于后续使用
                let socket_id = socket.id.to_string();

                // 注册客户端连接
                {
                    let subscriptions = subscriptions.clone();
                    let socket_id_clone = socket_id.clone();
                    tokio::spawn(async move {
                        let mut manager = subscriptions.write().await;
                        manager.connections.insert(
                            socket_id_clone.clone(),
                            ClientConnection {
                                socket_id: socket_id_clone,
                                subscriptions: HashSet::new(),
                                last_activity: Instant::now(),
                                connection_time: Instant::now(),
                                subscription_count: 0,
                                user_agent: None,
                                kline_data_sent_count: 0,
                                history_data_sent_count: 0,
                                total_messages_sent: 0,
                            },
                        );
                    });
                }

                // 发送连接成功消息
                let welcome_msg = serde_json::json!({
                    "client_id": socket_id,
                    "server_time": Utc::now().timestamp(),
                    "supported_symbols": [],
                    "supported_intervals": ["s1", "s30", "m5"]
                });

                if let Err(e) = socket.emit("connection_success", &welcome_msg) {
                    warn!("Failed to send welcome message: {}", e);
                }

                // 订阅事件处理器
                socket.on("subscribe", {
                    let subscriptions = subscriptions.clone();
                    let event_storage = event_storage.clone();

                    move |socket: SocketRef, Data(data): Data<SubscribeRequest>| {
                        let subscriptions = subscriptions.clone();
                        let event_storage = event_storage.clone();

                        tokio::spawn(async move {
                            info!(
                                "📊 Subscribe request from {}: {} {}",
                                socket.id, data.symbol, data.interval
                            );

                            // Update client activity
                            {
                                let mut manager = subscriptions.write().await;
                                manager.update_activity(&socket.id.to_string());
                            }

                            // 验证订阅请求
                            if let Err(e) = validate_subscribe_request(&data) {
                                let _ = socket.emit(
                                    "error",
                                    &serde_json::json!({
                                        "code": 1001,
                                        "message": e.to_string()
                                    }),
                                );
                                return;
                            }

                            // 添加订阅
                            {
                                let mut manager = subscriptions.write().await;
                                if let Err(e) = manager.add_subscription(
                                    &socket.id.to_string(),
                                    &data.symbol,
                                    &data.interval,
                                ) {
                                    let _ = socket.emit(
                                        "error",
                                        &serde_json::json!({
                                            "code": 1002,
                                            "message": e.to_string()
                                        }),
                                    );
                                    return;
                                }

                                // 更新活动时间
                                manager.update_activity(&socket.id.to_string());
                            }

                            // 加入对应的房间
                            let room_name = format!("kline:{}:{}", data.symbol, data.interval);
                            info!("🏠 Client {} joining room: {}", socket.id, room_name);
                            socket.join(room_name.clone());

                            // 检查订阅者状态
                            {
                                let manager = subscriptions.read().await;
                                let subscribers =
                                    manager.get_subscribers(&data.symbol, &data.interval);
                                info!(
                                    "📈 Current subscribers for {}:{}: {:?}",
                                    data.symbol, data.interval, subscribers
                                );
                                info!("📋 Total active connections: {}", manager.connections.len());
                            }

                            // 推送历史K线数据
                            if let Ok(history) =
                                get_kline_history(&event_storage, &data.symbol, &data.interval, 100)
                                    .await
                            {
                                if let Err(e) = socket.emit("history_data", &history) {
                                    warn!("Failed to send history data: {}", e);
                                } else {
                                    // 更新历史数据发送计数
                                    {
                                        let mut manager = subscriptions.write().await;
                                        if let Some(client) =
                                            manager.connections.get_mut(&socket.id.to_string())
                                        {
                                            client.history_data_sent_count += 1;
                                            client.total_messages_sent += 1;
                                        }
                                    }
                                }
                            }

                            // 推送历史交易事件数据 (300条)
                            info!("📡 Sending historical event data for mint: {}", data.symbol);
                            if let Ok(event_history) =
                                get_event_history(&event_storage, &data.symbol, 300).await
                            {
                                if let Err(e) = socket.emit("history_event_data", &event_history) {
                                    warn!("Failed to send history event data: {}", e);
                                } else {
                                    info!(
                                        "✅ Successfully sent {} historical events for mint: {}",
                                        event_history.data.len(),
                                        data.symbol
                                    );
                                    // 更新历史数据发送计数
                                    {
                                        let mut manager = subscriptions.write().await;
                                        if let Some(client) =
                                            manager.connections.get_mut(&socket.id.to_string())
                                        {
                                            client.history_data_sent_count += 1;
                                            client.total_messages_sent += 1;
                                        }
                                    }
                                }
                            } else {
                                warn!("❌ Failed to get historical event data for mint: {}", data.symbol);
                            }

                            // 确认订阅成功
                            let _ = socket.emit(
                                "subscription_confirmed",
                                &serde_json::json!({
                                    "symbol": data.symbol,
                                    "interval": data.interval,
                                    "subscription_id": data.subscription_id,
                                    "success": true,
                                    "message": "订阅成功"
                                }),
                            );
                        });
                    }
                });

                // 取消订阅事件处理器
                socket.on("unsubscribe", {
                    let subscriptions = subscriptions.clone();

                    move |socket: SocketRef, Data(data): Data<UnsubscribeRequest>| {
                        let subscriptions = subscriptions.clone();

                        tokio::spawn(async move {
                            info!(
                                "🚫 Unsubscribe request from {}: {} {}",
                                socket.id, data.symbol, data.interval
                            );

                            // 移除订阅
                            {
                                let mut manager = subscriptions.write().await;
                                manager.remove_subscription(
                                    &socket.id.to_string(),
                                    &data.symbol,
                                    &data.interval,
                                );
                                manager.update_activity(&socket.id.to_string());
                            }

                            // 离开对应的房间
                            let room_name = format!("kline:{}:{}", data.symbol, data.interval);
                            socket.leave(room_name);

                            // 确认取消订阅
                            let _ = socket.emit(
                                "unsubscribe_confirmed",
                                &serde_json::json!({
                                    "symbol": data.symbol,
                                    "interval": data.interval,
                                    "subscription_id": data.subscription_id,
                                    "success": true
                                }),
                            );
                        });
                    }
                });

                // 历史数据事件处理器
                socket.on("history", {
                    let event_storage = event_storage.clone();
                    let subscriptions = subscriptions.clone();

                    move |socket: SocketRef, Data(data): Data<HistoryRequest>| {
                        let event_storage = event_storage.clone();
                        let subscriptions = subscriptions.clone();

                        tokio::spawn(async move {
                            info!(
                                "📈 History request from {}: {} {}",
                                socket.id, data.symbol, data.interval
                            );

                            // 更新活动时间
                            {
                                let mut manager = subscriptions.write().await;
                                manager.update_activity(&socket.id.to_string());
                            }

                            match get_kline_history(
                                &event_storage,
                                &data.symbol,
                                &data.interval,
                                data.limit.unwrap_or(100),
                            )
                            .await
                            {
                                Ok(history) => {
                                    if let Err(e) = socket.emit("history_data", &history) {
                                        warn!("Failed to send history data: {}", e);
                                    } else {
                                        // 更新历史数据发送计数
                                        {
                                            let mut manager = subscriptions.write().await;
                                            if let Some(client) =
                                                manager.connections.get_mut(&socket.id.to_string())
                                            {
                                                client.history_data_sent_count += 1;
                                                client.total_messages_sent += 1;
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    let _ = socket.emit(
                                        "error",
                                        &serde_json::json!({
                                            "code": 1003,
                                            "message": e.to_string()
                                        }),
                                    );
                                }
                            }
                        });
                    }
                });

                // 连接断开事件处理器
                socket.on_disconnect({
                    let subscriptions = subscriptions.clone();

                    move |socket: SocketRef| {
                        let subscriptions = subscriptions.clone();

                        tokio::spawn(async move {
                            info!("🔌 Client disconnected: {}", socket.id);

                            // 清理客户端连接
                            let mut manager = subscriptions.write().await;
                            manager.remove_client(&socket.id.to_string());
                        });
                    }
                });
            }
        });
    }

    /// 广播交易事件到订阅者
    pub async fn broadcast_event_update(
        &self,
        event: &SpinPetEvent,
    ) -> Result<()> {
        let mint_account = self.get_mint_account_from_event(event);
        info!("📡 Broadcasting event update for mint: {}", mint_account);

        let event_type_name = get_event_type_name(event);
        let event_message = EventUpdateMessage {
            symbol: mint_account.clone(),
            event_type: event_type_name,
            event_data: event.clone(),
            timestamp: Utc::now().timestamp_millis() as u64,
        };

        // Use the same intervals as K-line push - broadcast to all possible intervals
        let intervals = ["s1", "s30", "m5"];
        let mut broadcast_count = 0;

        for interval in intervals {
            let room_name = format!("kline:{}:{}", mint_account, interval);
            
            let result = self
                .socketio
                .of("/kline")
                .ok_or_else(|| anyhow::anyhow!("Namespace /kline not found"))?
                .to(room_name.clone())
                .emit("event_data", &event_message)
                .await;

            match result {
                Ok(_) => {
                    info!("✅ Successfully broadcasted event to room {}", room_name);
                    broadcast_count += 1;
                }
                Err(e) => {
                    warn!("❌ Failed to broadcast event to room {}: {}", room_name, e);
                }
            }
        }

        info!("📡 Event broadcast completed for mint: {}, sent to {} rooms", mint_account, broadcast_count);
        Ok(())
    }

    /// 从事件中获取 mint_account
    fn get_mint_account_from_event(&self, event: &SpinPetEvent) -> String {
        match event {
            SpinPetEvent::TokenCreated(e) => e.mint_account.clone(),
            SpinPetEvent::BuySell(e) => e.mint_account.clone(),
            SpinPetEvent::LongShort(e) => e.mint_account.clone(),
            SpinPetEvent::ForceLiquidate(e) => e.mint_account.clone(),
            SpinPetEvent::FullClose(e) => e.mint_account.clone(),
            SpinPetEvent::PartialClose(e) => e.mint_account.clone(),
            SpinPetEvent::MilestoneDiscount(e) => e.mint_account.clone(),
        }
    }

    /// 广播K线更新到订阅者
    pub async fn broadcast_kline_update(
        &self,
        mint_account: &str,
        interval: &str,
        kline_data: &KlineData,
    ) -> Result<()> {
        let room_name = format!("kline:{}:{}", mint_account, interval);

        let update_message = KlineUpdateMessage {
            symbol: mint_account.to_string(),
            interval: interval.to_string(),
            subscription_id: None,
            data: KlineRealtimeData {
                time: kline_data.time,
                open: kline_data.open,
                high: kline_data.high,
                low: kline_data.low,
                close: kline_data.close,
                volume: kline_data.volume,
                is_final: kline_data.is_final,
                update_type: if kline_data.is_final {
                    "final".to_string()
                } else {
                    "realtime".to_string()
                },
                update_count: kline_data.update_count,
            },
            timestamp: Utc::now().timestamp_millis() as u64,
        };

        info!("📡 Broadcasting kline update to room: {}", room_name);
        info!("📊 Update message: time={}, open={}, high={}, low={}, close={}, volume={}, is_final={}, update_count={}",
            update_message.data.time, update_message.data.open, update_message.data.high,
            update_message.data.low, update_message.data.close, update_message.data.volume,
            update_message.data.is_final, update_message.data.update_count);

        // 在发送前检查房间中的实际连接
        {
            let manager = self.subscriptions.read().await;
            let subscribers = manager.get_subscribers(mint_account, interval);
            info!(
                "📋 Room {} has {} subscribers: {:?}",
                room_name,
                subscribers.len(),
                subscribers
            );
        }

        // 发送到 /kline 命名空间的房间
        let result = self
            .socketio
            .of("/kline")
            .ok_or_else(|| anyhow::anyhow!("Namespace /kline not found"))?
            .to(room_name.clone())
            .emit("kline_data", &update_message)
            .await;

        match result {
            Ok(_) => {
                info!(
                    "✅ Successfully broadcasted kline update to room {}",
                    room_name
                );

                // 验证消息确实发送到了客户端 - 尝试直接发送到socket
                {
                    let manager = self.subscriptions.read().await;
                    let subscribers = manager.get_subscribers(mint_account, interval);
                    info!(
                        "🔍 Attempting direct send to {} subscribers",
                        subscribers.len()
                    );

                    for socket_id in &subscribers {
                        // 尝试直接发送给特定socket (在 /kline 命名空间中)
                        if let Some(ns) = self.socketio.of("/kline") {
                            if let Err(e) = ns
                                .to(socket_id.clone())
                                .emit("direct_kline_test", &update_message)
                                .await
                            {
                                warn!(
                                    "❌ Failed to send direct test to socket {}: {}",
                                    socket_id, e
                                );
                            } else {
                                info!("✅ Direct test sent to socket {}", socket_id);
                            }
                        }
                    }
                }

                // 更新所有订阅了该房间的客户端的 kline_data 发送计数
                {
                    let mut manager = self.subscriptions.write().await;
                    let subscribers = manager.get_subscribers(mint_account, interval);
                    for socket_id in subscribers {
                        if let Some(client) = manager.connections.get_mut(&socket_id) {
                            client.kline_data_sent_count += 1;
                            client.total_messages_sent += 1;
                        }
                    }
                }
            }
            Err(e) => {
                warn!("❌ Failed to broadcast to room {}: {}", room_name, e);
            }
        }

        Ok(())
    }

    /// 获取服务统计信息
    pub async fn get_service_stats(&self) -> serde_json::Value {
        let manager = self.subscriptions.read().await;

        serde_json::json!({
            "active_connections": manager.connections.len(),
            "total_subscriptions": manager.client_subscriptions.values().map(|s| s.len()).sum::<usize>(),
            "monitored_mints": manager.mint_subscribers.len(),
            "config": {
                "connection_timeout": self.config.connection_timeout.as_secs(),
                "max_subscriptions_per_client": self.config.max_subscriptions_per_client,
                "ping_interval": self.config.ping_interval.as_secs(),
                "ping_timeout": self.config.ping_timeout.as_secs()
            }
        })
    }

    /// 获取详细的订阅状态和通讯统计
    pub async fn get_subscription_details(&self) -> serde_json::Value {
        let manager = self.subscriptions.read().await;
        let now = Instant::now();

        let mut client_details = Vec::new();

        for (socket_id, client) in &manager.connections {
            let subscriptions: Vec<String> = client.subscriptions.iter().cloned().collect();
            let connection_duration = now.duration_since(client.connection_time).as_secs();
            let last_activity_ago = now.duration_since(client.last_activity).as_secs();

            client_details.push(serde_json::json!({
                "socket_id": socket_id,
                "subscriptions": subscriptions,
                "subscription_count": client.subscription_count,
                "connection_duration_seconds": connection_duration,
                "last_activity_seconds_ago": last_activity_ago,
                "message_stats": {
                    "kline_data_sent": client.kline_data_sent_count,
                    "history_data_sent": client.history_data_sent_count,
                    "total_messages_sent": client.total_messages_sent
                }
            }));
        }

        let mut room_details = Vec::new();

        for (mint, intervals) in &manager.mint_subscribers {
            for (interval, subscribers) in intervals {
                let room_name = format!("kline:{}:{}", mint, interval);
                room_details.push(serde_json::json!({
                    "room_name": room_name,
                    "mint": mint,
                    "interval": interval,
                    "subscriber_count": subscribers.len(),
                    "subscribers": subscribers.iter().cloned().collect::<Vec<String>>()
                }));
            }
        }

        serde_json::json!({
            "timestamp": chrono::Utc::now().timestamp(),
            "total_connections": manager.connections.len(),
            "total_rooms": room_details.len(),
            "clients": client_details,
            "rooms": room_details
        })
    }
}

/// 验证订阅请求
fn validate_subscribe_request(req: &SubscribeRequest) -> Result<()> {
    // 验证时间间隔
    if !["s1", "s30", "m5"].contains(&req.interval.as_str()) {
        return Err(anyhow::anyhow!(
            "Invalid interval: {}, must be one of: s1, s30, m5",
            req.interval
        ));
    }

    // 验证symbol格式（基本的Solana地址格式检查）
    if req.symbol.len() < 32 || req.symbol.len() > 44 {
        return Err(anyhow::anyhow!("Invalid symbol format"));
    }

    Ok(())
}

/// 获取历史K线数据
async fn get_kline_history(
    event_storage: &Arc<EventStorage>,
    symbol: &str,
    interval: &str,
    limit: usize,
) -> Result<KlineHistoryResponse> {
    let query = KlineQuery {
        mint_account: symbol.to_string(),
        interval: interval.to_string(),
        page: Some(1),
        limit: Some(limit),
        order_by: Some("time_desc".to_string()),
    };

    let response = event_storage.query_kline_data(query).await?;

    let data: Vec<KlineRealtimeData> = response
        .klines
        .into_iter()
        .map(|kline| KlineRealtimeData {
            time: kline.time,
            open: kline.open,
            high: kline.high,
            low: kline.low,
            close: kline.close,
            volume: kline.volume,
            is_final: kline.is_final,
            update_type: if kline.is_final {
                "final".to_string()
            } else {
                "realtime".to_string()
            },
            update_count: kline.update_count,
        })
        .collect();

    Ok(KlineHistoryResponse {
        symbol: symbol.to_string(),
        interval: interval.to_string(),
        data,
        has_more: response.has_next,
        total_count: response.total,
    })
}

/// 获取历史交易事件数据
async fn get_event_history(
    event_storage: &Arc<EventStorage>,
    symbol: &str,
    limit: usize,
) -> Result<EventHistoryResponse> {
    use crate::services::event_storage::EventQuery;
    
    let query = EventQuery {
        mint_account: symbol.to_string(),
        page: Some(1),
        limit: Some(limit),
        order_by: Some("slot_desc".to_string()), // slot 从大到小排列
    };

    let response = event_storage.query_events(query).await?;

    let data: Vec<EventUpdateMessage> = response
        .events
        .into_iter()
        .map(|event| {
            let event_type_name = get_event_type_name(&event);
            EventUpdateMessage {
                symbol: symbol.to_string(),
                event_type: event_type_name,
                event_data: event,
                timestamp: Utc::now().timestamp_millis() as u64,
            }
        })
        .collect();

    Ok(EventHistoryResponse {
        symbol: symbol.to_string(),
        data,
        has_more: response.has_next,
        total_count: response.total,
    })
}

/// 获取事件类型名称
fn get_event_type_name(event: &SpinPetEvent) -> String {
    match event {
        SpinPetEvent::TokenCreated(_) => "TokenCreated".to_string(),
        SpinPetEvent::BuySell(_) => "BuySell".to_string(),
        SpinPetEvent::LongShort(_) => "LongShort".to_string(),
        SpinPetEvent::ForceLiquidate(_) => "ForceLiquidate".to_string(),
        SpinPetEvent::FullClose(_) => "FullClose".to_string(),
        SpinPetEvent::PartialClose(_) => "PartialClose".to_string(),
        SpinPetEvent::MilestoneDiscount(_) => "MilestoneDiscount".to_string(),
    }
}

/// 连接清理任务
pub async fn start_connection_cleanup_task(
    subscriptions: Arc<RwLock<SubscriptionManager>>,
    config: KlineConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30)); // 每30秒清理一次

        loop {
            interval.tick().await;

            let now = Instant::now();
            let inactive_clients: Vec<String>;

            // 查找超时的连接
            {
                let manager = subscriptions.read().await;
                inactive_clients = manager
                    .connections
                    .iter()
                    .filter(|(_, conn)| {
                        now.duration_since(conn.last_activity) > config.connection_timeout
                    })
                    .map(|(id, _)| id.clone())
                    .collect();
            }

            // 清理超时连接
            if !inactive_clients.is_empty() {
                let mut manager = subscriptions.write().await;
                for socket_id in inactive_clients {
                    manager.remove_client(&socket_id);
                    info!("🧹 Cleaned up inactive connection: {}", socket_id);
                }
            }

            // 记录统计信息
            let manager = subscriptions.read().await;
            debug!(
                "📊 Active connections: {}, Total subscriptions: {}",
                manager.connections.len(),
                manager
                    .client_subscriptions
                    .values()
                    .map(|s| s.len())
                    .sum::<usize>()
            );
        }
    })
}

/// 性能监控任务
pub async fn start_performance_monitoring_task(
    subscriptions: Arc<RwLock<SubscriptionManager>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60)); // 每分钟记录一次

        loop {
            interval.tick().await;

            let manager = subscriptions.read().await;
            let connection_count = manager.connections.len();
            let subscription_count: usize =
                manager.client_subscriptions.values().map(|s| s.len()).sum();
            let mint_count = manager.mint_subscribers.len();

            info!(
                "📊 Kline Service Metrics - Connections: {}, Subscriptions: {}, Monitored Mints: {}",
                connection_count, subscription_count, mint_count
            );

            // 记录最活跃的 mint
            let top_mints: Vec<_> = manager
                .mint_subscribers
                .iter()
                .map(|(mint, intervals)| {
                    let total_subscribers: usize = intervals.values().map(|s| s.len()).sum();
                    (mint.clone(), total_subscribers)
                })
                .collect();

            if !top_mints.is_empty() {
                let mut sorted_mints = top_mints;
                sorted_mints.sort_by(|a, b| b.1.cmp(&a.1));

                let top_5: Vec<String> = sorted_mints
                    .into_iter()
                    .take(5)
                    .map(|(mint, count)| format!("{}({})", &mint[..8.min(mint.len())], count))
                    .collect();

                debug!("🔥 Top mints by subscribers: {}", top_5.join(", "));
            }
        }
    })
}

/// 扩展的事件处理器，增加K线实时推送功能
pub struct KlineEventHandler {
    pub stats_handler: Arc<StatsEventHandler>,
    pub kline_service: Arc<KlineSocketService>,
}

impl KlineEventHandler {
    pub fn new(
        stats_handler: Arc<StatsEventHandler>,
        kline_service: Arc<KlineSocketService>,
    ) -> Self {
        Self {
            stats_handler,
            kline_service,
        }
    }

    /// 提取事件中的价格信息
    fn extract_price_info(&self, event: &SpinPetEvent) -> Option<(String, u128, DateTime<Utc>)> {
        match event {
            SpinPetEvent::BuySell(e) => Some((e.mint_account.clone(), e.latest_price, e.timestamp)),
            SpinPetEvent::LongShort(e) => {
                Some((e.mint_account.clone(), e.latest_price, e.timestamp))
            }
            SpinPetEvent::FullClose(e) => {
                Some((e.mint_account.clone(), e.latest_price, e.timestamp))
            }
            SpinPetEvent::PartialClose(e) => {
                Some((e.mint_account.clone(), e.latest_price, e.timestamp))
            }
            _ => None, // TokenCreated、ForceLiquidate、MilestoneDiscount 不包含价格信息
        }
    }

    /// 触发K线数据推送
    async fn trigger_kline_push(
        &self,
        mint_account: &str,
        latest_price: u128,
        timestamp: DateTime<Utc>,
    ) -> Result<()> {
        info!(
            "🔔 Triggering kline push for mint: {}, price: {}, timestamp: {}",
            mint_account, latest_price, timestamp
        );
        let intervals = ["s1", "s30", "m5"];

        for interval in intervals {
            info!(
                "📊 Processing interval: {} for mint: {}",
                interval, mint_account
            );
            // 获取更新后的K线数据（从现有存储中读取）
            match self
                .get_latest_kline(mint_account, interval, timestamp)
                .await
            {
                Ok(kline_data) => {
                    info!(
                        "✅ Found kline data for {}:{} - time: {}, price: {}",
                        mint_account, interval, kline_data.time, kline_data.close
                    );
                    // 使用 KlineSocketService 广播到对应房间
                    if let Err(e) = self
                        .kline_service
                        .broadcast_kline_update(mint_account, interval, &kline_data)
                        .await
                    {
                        warn!("❌ Failed to broadcast kline update: {}", e);
                    } else {
                        info!(
                            "📡 Successfully broadcasted kline update for {}:{}",
                            mint_account, interval
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "⚠️ No kline data found for {}:{} - {}",
                        mint_account, interval, e
                    );
                }
            }
        }

        Ok(())
    }

    /// 获取最新K线数据
    async fn get_latest_kline(
        &self,
        mint_account: &str,
        interval: &str,
        _timestamp: DateTime<Utc>,
    ) -> Result<KlineData> {
        // 从现有的 EventStorage 查询K线数据
        let query = KlineQuery {
            mint_account: mint_account.to_string(),
            interval: interval.to_string(),
            page: Some(1),
            limit: Some(1),
            order_by: Some("time_desc".to_string()),
        };

        let response = self
            .kline_service
            .event_storage
            .query_kline_data(query)
            .await?;

        if let Some(kline) = response.klines.first() {
            Ok(kline.clone())
        } else {
            Err(anyhow::anyhow!("No kline data found"))
        }
    }
}

#[async_trait::async_trait]
impl EventHandler for KlineEventHandler {
    async fn handle_event(&self, event: SpinPetEvent) -> anyhow::Result<()> {
        info!("🎯 KlineEventHandler received event: {:?}", event);

        // 1. 调用现有的统计和存储逻辑
        self.stats_handler.handle_event(event.clone()).await?;

        // 2. 实时推送交易事件给订阅者
        info!("📡 Broadcasting event to subscribers: {:?}", event);
        if let Err(e) = self.kline_service.broadcast_event_update(&event).await {
            warn!("❌ Failed to broadcast event update: {}", e);
        } else {
            info!("✅ Successfully broadcasted event update");
        }

        // 3. 提取价格信息并触发K线推送
        if let Some((mint_account, latest_price, timestamp)) = self.extract_price_info(&event) {
            info!(
                "💰 Extracted price info: mint={}, price={}, timestamp={}",
                mint_account, latest_price, timestamp
            );
            if let Err(e) = self
                .trigger_kline_push(&mint_account, latest_price, timestamp)
                .await
            {
                warn!(
                    "❌ Failed to trigger kline push for {}: {}",
                    mint_account, e
                );
            } else {
                info!(
                    "✅ Successfully triggered kline push for {} at price {}",
                    mint_account, latest_price
                );
            }
        } else {
            debug!(
                "🚫 Event does not contain price info, skipping kline push: {:?}",
                event
            );
        }

        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// 包含测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Config, CorsConfig, DatabaseConfig, IpfsConfig, KlineServiceConfig, LoggingConfig,
        ServerConfig, SolanaConfig,
    };
    use std::time::Duration;
    use tempfile::TempDir;

    fn create_test_config() -> Config {
        let temp_dir = TempDir::new().unwrap();
        Config {
            server: ServerConfig {
                host: "localhost".to_string(),
                port: 8080,
            },
            cors: CorsConfig {
                enabled: true,
                allow_origins: vec!["*".to_string()],
            },
            logging: LoggingConfig {
                level: "debug".to_string(),
            },
            solana: SolanaConfig {
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
                process_failed_transactions: true,
            },
            database: DatabaseConfig {
                rocksdb_path: temp_dir.path().to_str().unwrap().to_string(),
            },
            ipfs: IpfsConfig {
                gateway_url: "https://gateway.pinata.cloud/ipfs/".to_string(),
                request_timeout_seconds: 30,
                max_retries: 3,
                retry_delay_seconds: 5,
            },
            kline: KlineServiceConfig {
                enable_kline_service: true,
                connection_timeout_secs: 60,
                max_subscriptions_per_client: 100,
                history_data_limit: 100,
                ping_interval_secs: 25,
                ping_timeout_secs: 60,
            },
        }
    }

    #[test]
    fn test_kline_config_creation() {
        let config = create_test_config();
        let kline_config = KlineConfig::from_config(&config.kline);

        assert_eq!(kline_config.connection_timeout, Duration::from_secs(60));
        assert_eq!(kline_config.max_subscriptions_per_client, 100);
        assert_eq!(kline_config.ping_interval, Duration::from_secs(25));
        assert_eq!(kline_config.ping_timeout, Duration::from_secs(60));
    }

    #[test]
    fn test_subscription_manager() {
        let mut manager = SubscriptionManager::new();

        // 模拟客户端连接
        let socket_id = "test_socket_123";
        manager.connections.insert(
            socket_id.to_string(),
            ClientConnection {
                socket_id: socket_id.to_string(),
                subscriptions: HashSet::new(),
                last_activity: Instant::now(),
                connection_time: Instant::now(),
                subscription_count: 0,
                user_agent: Some("test_client".to_string()),
                kline_data_sent_count: 0,
                history_data_sent_count: 0,
                total_messages_sent: 0,
            },
        );

        // 测试添加订阅
        let result = manager.add_subscription(socket_id, "test_mint", "s1");
        assert!(result.is_ok());

        // 验证订阅已添加
        assert_eq!(manager.connections[socket_id].subscription_count, 1);
        assert!(manager.connections[socket_id]
            .subscriptions
            .contains("test_mint:s1"));

        // 测试获取订阅者
        let subscribers = manager.get_subscribers("test_mint", "s1");
        assert_eq!(subscribers.len(), 1);
        assert_eq!(subscribers[0], socket_id);

        // 测试移除订阅
        manager.remove_subscription(socket_id, "test_mint", "s1");
        assert_eq!(manager.connections[socket_id].subscription_count, 0);
        assert!(!manager.connections[socket_id]
            .subscriptions
            .contains("test_mint:s1"));

        // 测试清理客户端
        manager.remove_client(socket_id);
        assert!(!manager.connections.contains_key(socket_id));
    }

    #[test]
    fn test_subscription_limit() {
        let mut manager = SubscriptionManager::new();

        // 模拟客户端连接
        let socket_id = "test_socket_456";
        manager.connections.insert(
            socket_id.to_string(),
            ClientConnection {
                socket_id: socket_id.to_string(),
                subscriptions: HashSet::new(),
                last_activity: Instant::now(),
                connection_time: Instant::now(),
                subscription_count: 100, // 已达到限制
                user_agent: Some("test_client".to_string()),
                kline_data_sent_count: 0,
                history_data_sent_count: 0,
                total_messages_sent: 0,
            },
        );

        // 尝试添加超出限制的订阅
        let result = manager.add_subscription(socket_id, "test_mint", "s1");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Subscription limit exceeded"));
    }

    #[test]
    fn test_validate_subscribe_request() {
        // 有效请求
        let valid_request = SubscribeRequest {
            symbol: "JBMmrp6jhksqnxDBskkmVvWHhJLaPBjgiMHEroJbUTBZ".to_string(),
            interval: "s1".to_string(),
            subscription_id: Some("test_123".to_string()),
        };
        assert!(validate_subscribe_request(&valid_request).is_ok());

        // 无效间隔
        let invalid_interval = SubscribeRequest {
            symbol: "JBMmrp6jhksqnxDBskkmVvWHhJLaPBjgiMHEroJbUTBZ".to_string(),
            interval: "invalid".to_string(),
            subscription_id: Some("test_123".to_string()),
        };
        assert!(validate_subscribe_request(&invalid_interval).is_err());

        // 无效符号格式
        let invalid_symbol = SubscribeRequest {
            symbol: "short".to_string(), // 太短
            interval: "s1".to_string(),
            subscription_id: Some("test_123".to_string()),
        };
        assert!(validate_subscribe_request(&invalid_symbol).is_err());
    }

    #[tokio::test]
    async fn test_kline_socket_service_creation() {
        let config = create_test_config();
        let event_storage = Arc::new(EventStorage::new(&config).unwrap());
        let kline_config = KlineConfig::from_config(&config.kline);

        let result = KlineSocketService::new(event_storage, kline_config);
        assert!(result.is_ok());

        let (service, _layer) = result.unwrap();
        let stats = service.get_service_stats().await;

        // 验证初始统计信息
        assert_eq!(stats["active_connections"], 0);
        assert_eq!(stats["total_subscriptions"], 0);
        assert_eq!(stats["monitored_mints"], 0);
    }

    #[test]
    fn test_kline_data_conversion() {
        let original_kline = KlineData {
            time: 1234567890,
            open: 1.23,
            high: 1.45,
            low: 1.10,
            close: 1.35,
            volume: 0.0,
            is_final: false,
            update_count: 5,
        };

        let realtime_data = KlineRealtimeData {
            time: original_kline.time,
            open: original_kline.open,
            high: original_kline.high,
            low: original_kline.low,
            close: original_kline.close,
            volume: original_kline.volume,
            is_final: original_kline.is_final,
            update_type: if original_kline.is_final {
                "final".to_string()
            } else {
                "realtime".to_string()
            },
            update_count: original_kline.update_count,
        };

        assert_eq!(realtime_data.time, original_kline.time);
        assert_eq!(realtime_data.close, original_kline.close);
        assert_eq!(realtime_data.update_type, "realtime");
        assert_eq!(realtime_data.update_count, 5);
    }
}

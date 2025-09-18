// 正确的 socketioxide setup_socket_handlers 实现
pub fn setup_socket_handlers(&self) {
    let subscriptions = Arc::clone(&self.subscriptions);
    let event_storage = Arc::clone(&self.event_storage);
    
    // 设置默认命名空间（避免default namespace not found错误）
    self.socketio.ns("/", |_socket: SocketRef| {
        // 默认命名空间不做任何处理，只是为了避免错误
    });
    
    // K线命名空间
    self.socketio.ns("/kline", {
        let subscriptions = subscriptions.clone();
        let event_storage = event_storage.clone();
        
        move |socket: SocketRef| {
            info!("🔌 New client connected to /kline: {}", socket.id);
            
            // 注册客户端连接
            {
                let subscriptions = subscriptions.clone();
                tokio::spawn(async move {
                    let mut manager = subscriptions.write().await;
                    manager.connections.insert(socket.id.to_string(), ClientConnection {
                        socket_id: socket.id.to_string(),
                        subscriptions: HashSet::new(),
                        last_activity: Instant::now(),
                        connection_time: Instant::now(),
                        subscription_count: 0,
                        user_agent: None,
                    });
                });
            }
            
            // 发送连接成功消息
            let welcome_msg = serde_json::json!({
                "client_id": socket.id.to_string(),
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
                        info!("📊 Subscribe request from {}: {} {}", socket.id, data.symbol, data.interval);
                        
                        // 验证订阅请求
                        if let Err(e) = validate_subscribe_request(&data) {
                            let _ = socket.emit("error", &serde_json::json!({
                                "code": 1001,
                                "message": e.to_string()
                            }));
                            return;
                        }
                        
                        // 添加订阅
                        {
                            let mut manager = subscriptions.write().await;
                            if let Err(e) = manager.add_subscription(&socket.id.to_string(), &data.symbol, &data.interval) {
                                let _ = socket.emit("error", &serde_json::json!({
                                    "code": 1002,
                                    "message": e.to_string()
                                }));
                                return;
                            }
                            
                            // 更新活动时间
                            manager.update_activity(&socket.id.to_string());
                        }
                        
                        // 加入对应的房间
                        let room_name = format!("kline:{}:{}", data.symbol, data.interval);
                        socket.join(room_name);
                        
                        // 推送历史数据
                        if let Ok(history) = get_kline_history(&event_storage, &data.symbol, &data.interval, 100).await {
                            if let Err(e) = socket.emit("history_data", &history) {
                                warn!("Failed to send history data: {}", e);
                            }
                        }
                        
                        // 确认订阅成功
                        let _ = socket.emit("subscription_confirmed", &serde_json::json!({
                            "symbol": data.symbol,
                            "interval": data.interval,
                            "subscription_id": data.subscription_id,
                            "success": true,
                            "message": "订阅成功"
                        }));
                    });
                }
            });
            
            // 取消订阅事件处理器
            socket.on("unsubscribe", {
                let subscriptions = subscriptions.clone();
                
                move |socket: SocketRef, Data(data): Data<UnsubscribeRequest>| {
                    let subscriptions = subscriptions.clone();
                    
                    tokio::spawn(async move {
                        info!("🚫 Unsubscribe request from {}: {} {}", socket.id, data.symbol, data.interval);
                        
                        // 移除订阅
                        {
                            let mut manager = subscriptions.write().await;
                            manager.remove_subscription(&socket.id.to_string(), &data.symbol, &data.interval);
                            manager.update_activity(&socket.id.to_string());
                        }
                        
                        // 离开对应的房间
                        let room_name = format!("kline:{}:{}", data.symbol, data.interval);
                        socket.leave(room_name);
                        
                        // 确认取消订阅
                        let _ = socket.emit("unsubscribe_confirmed", &serde_json::json!({
                            "symbol": data.symbol,
                            "interval": data.interval,
                            "subscription_id": data.subscription_id,
                            "success": true
                        }));
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
                        info!("📈 History request from {}: {} {}", socket.id, data.symbol, data.interval);
                        
                        // 更新活动时间
                        {
                            let mut manager = subscriptions.write().await;
                            manager.update_activity(&socket.id.to_string());
                        }
                        
                        match get_kline_history(&event_storage, &data.symbol, &data.interval, data.limit.unwrap_or(100)).await {
                            Ok(history) => {
                                if let Err(e) = socket.emit("history_data", &history) {
                                    warn!("Failed to send history data: {}", e);
                                }
                            }
                            Err(e) => {
                                let _ = socket.emit("error", &serde_json::json!({
                                    "code": 1003,
                                    "message": e.to_string()
                                }));
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
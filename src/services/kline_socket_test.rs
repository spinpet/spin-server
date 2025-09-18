#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tempfile::TempDir;
    use crate::config::{Config, KlineServiceConfig, DatabaseConfig, SolanaConfig, ServerConfig, CorsConfig, LoggingConfig, IpfsConfig};

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
        manager.connections.insert(socket_id.to_string(), ClientConnection {
            socket_id: socket_id.to_string(),
            subscriptions: HashSet::new(),
            last_activity: Instant::now(),
            connection_time: Instant::now(),
            subscription_count: 0,
            user_agent: Some("test_client".to_string()),
        });

        // 测试添加订阅
        let result = manager.add_subscription(socket_id, "test_mint", "s1");
        assert!(result.is_ok());
        
        // 验证订阅已添加
        assert_eq!(manager.connections[socket_id].subscription_count, 1);
        assert!(manager.connections[socket_id].subscriptions.contains("test_mint:s1"));
        
        // 测试获取订阅者
        let subscribers = manager.get_subscribers("test_mint", "s1");
        assert_eq!(subscribers.len(), 1);
        assert_eq!(subscribers[0], socket_id);
        
        // 测试移除订阅
        manager.remove_subscription(socket_id, "test_mint", "s1");
        assert_eq!(manager.connections[socket_id].subscription_count, 0);
        assert!(!manager.connections[socket_id].subscriptions.contains("test_mint:s1"));
        
        // 测试清理客户端
        manager.remove_client(socket_id);
        assert!(!manager.connections.contains_key(socket_id));
    }

    #[test]
    fn test_subscription_limit() {
        let mut manager = SubscriptionManager::new();
        
        // 模拟客户端连接
        let socket_id = "test_socket_456";
        manager.connections.insert(socket_id.to_string(), ClientConnection {
            socket_id: socket_id.to_string(),
            subscriptions: HashSet::new(),
            last_activity: Instant::now(),
            connection_time: Instant::now(),
            subscription_count: 100, // 已达到限制
            user_agent: Some("test_client".to_string()),
        });

        // 尝试添加超出限制的订阅
        let result = manager.add_subscription(socket_id, "test_mint", "s1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Subscription limit exceeded"));
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
            update_type: if original_kline.is_final { "final".to_string() } else { "realtime".to_string() },
            update_count: original_kline.update_count,
        };

        assert_eq!(realtime_data.time, original_kline.time);
        assert_eq!(realtime_data.close, original_kline.close);
        assert_eq!(realtime_data.update_type, "realtime");
        assert_eq!(realtime_data.update_count, 5);
    }
}
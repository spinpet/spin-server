use crate::solana::{
    SolanaClient, EventListenerManager, DefaultEventHandler, EventHandler, SpinPetEvent
};
use crate::config::{SolanaConfig, DatabaseConfig};
use crate::services::event_storage::EventStorage;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use utoipa::ToSchema;

/// Event service status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventServiceStatus {
    pub is_running: bool,
    #[schema(value_type = Option<String>)]
    pub last_event_time: Option<DateTime<Utc>>,
    pub total_events_processed: u64,
    pub connection_status: String,
    pub program_id: String,
}

/// Event statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct EventStats {
    pub token_created: u64,
    pub buy_sell: u64,
    pub long_short: u64,
    pub force_liquidate: u64,
    pub full_close: u64,
    pub partial_close: u64,
    pub total: u64,
}

/// Enhanced event handler with statistics and storage functionality
pub struct StatsEventHandler {
    stats: Arc<RwLock<EventStats>>,
    last_event_time: Arc<RwLock<Option<DateTime<Utc>>>>,
    event_storage: Arc<EventStorage>,
}

impl StatsEventHandler {
    pub fn new(event_storage: Arc<EventStorage>) -> Self {
        Self {
            stats: Arc::new(RwLock::new(EventStats {
                token_created: 0,
                buy_sell: 0,
                long_short: 0,
                force_liquidate: 0,
                full_close: 0,
                partial_close: 0,
                total: 0,
            })),
            last_event_time: Arc::new(RwLock::new(None)),
            event_storage,
        }
    }

    pub async fn get_stats(&self) -> EventStats {
        self.stats.read().await.clone()
    }

    pub async fn get_last_event_time(&self) -> Option<DateTime<Utc>> {
        *self.last_event_time.read().await
    }
}

#[async_trait::async_trait]
impl EventHandler for StatsEventHandler {
    async fn handle_event(&self, event: SpinPetEvent) -> anyhow::Result<()> {
        // Store event in RocksDB
        if let Err(e) = self.event_storage.store_event(event.clone()).await {
            error!("❌ Failed to store event: {}", e);
            // Don't block processing, just log the error
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            match &event {
                SpinPetEvent::TokenCreated(_) => stats.token_created += 1,
                SpinPetEvent::BuySell(_) => stats.buy_sell += 1,
                SpinPetEvent::LongShort(_) => stats.long_short += 1,
                SpinPetEvent::ForceLiquidate(_) => stats.force_liquidate += 1,
                SpinPetEvent::FullClose(_) => stats.full_close += 1,
                SpinPetEvent::PartialClose(_) => stats.partial_close += 1,
            }
            stats.total += 1;
        }

        // Update last event time
        {
            let mut last_time = self.last_event_time.write().await;
            *last_time = Some(Utc::now());
        }

        // Call default handler for log output
        let default_handler = DefaultEventHandler;
        default_handler.handle_event(event).await?;

        Ok(())
    }
}

/// Event service manager
pub struct EventService {
    client: Arc<SolanaClient>,
    listener_manager: EventListenerManager,
    event_handler: Arc<StatsEventHandler>,
    event_storage: Arc<EventStorage>,
    config: SolanaConfig,
}

impl EventService {
    /// Create a new event service
    pub fn new(config: SolanaConfig, database_config: DatabaseConfig) -> anyhow::Result<Self> {
        let client = Arc::new(SolanaClient::new(&config.rpc_url, &config.program_id)?);
        let event_storage = Arc::new(EventStorage::new(&database_config)?);
        let event_handler = Arc::new(StatsEventHandler::new(Arc::clone(&event_storage)));
        let mut listener_manager = EventListenerManager::new();
        
        // Initialize listener
        listener_manager.initialize(
            config.clone(),
            Arc::clone(&client),
            Arc::clone(&event_handler) as Arc<dyn EventHandler>,
        )?;

        Ok(Self {
            client,
            listener_manager,
            event_handler,
            event_storage,
            config,
        })
    }

    /// Start event service
    pub async fn start(&mut self) -> anyhow::Result<()> {
        if !self.config.enable_event_listener {
            info!("Event listener is disabled");
            return Ok(());
        }

        info!("🚀 Starting event service");
        
        // Check Solana connection
        if !self.client.check_connection().await? {
            return Err(anyhow::anyhow!("Unable to connect to Solana network"));
        }

        // Start listener
        self.listener_manager.start().await?;
        
        info!("✅ Event service started successfully");
        Ok(())
    }

    /// Stop event service
    pub async fn stop(&mut self) -> anyhow::Result<()> {
        info!("🛑 Stopping event service");
        self.listener_manager.stop().await?;
        info!("✅ Event service stopped");
        Ok(())
    }

    /// Get service status
    pub async fn get_status(&self) -> EventServiceStatus {
        let stats = self.event_handler.get_stats().await;
        let last_event_time = self.event_handler.get_last_event_time().await;
        
        let connection_status = match self.client.check_connection().await {
            Ok(true) => "Connected".to_string(),
            Ok(false) => "Connection failed".to_string(),
            Err(e) => format!("Connection error: {}", e),
        };

        EventServiceStatus {
            is_running: self.listener_manager.is_running(),
            last_event_time,
            total_events_processed: stats.total,
            connection_status,
            program_id: self.config.program_id.clone(),
        }
    }

    /// Get event statistics
    pub async fn get_stats(&self) -> EventStats {
        self.event_handler.get_stats().await
    }

    /// Check if service is running
    pub fn is_running(&self) -> bool {
        self.listener_manager.is_running()
    }

    /// Get program ID
    pub fn get_program_id(&self) -> &str {
        &self.config.program_id
    }

    /// Get event storage
    pub fn get_event_storage(&self) -> Arc<EventStorage> {
        Arc::clone(&self.event_storage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_stats_creation() {
        let stats = EventStats {
            token_created: 0,
            buy_sell: 0,
            long_short: 0,
            force_liquidate: 0,
            full_close: 0,
            partial_close: 0,
            total: 0,
        };
        
        assert_eq!(stats.total, 0);
    }

    #[tokio::test]
    async fn test_stats_event_handler() {
        // This is just a test stub, in real code we need to provide event_storage
        // Create a mock storage for testing
        use tempfile::TempDir;
        use crate::config::DatabaseConfig;
        
        let temp_dir = TempDir::new().unwrap();
        let config = DatabaseConfig {
            rocksdb_path: temp_dir.path().to_str().unwrap().to_string(),
        };
        let event_storage = Arc::new(EventStorage::new(&config).unwrap());
        
        let handler = StatsEventHandler::new(event_storage);
        let initial_stats = handler.get_stats().await;
        
        assert_eq!(initial_stats.total, 0);
        assert!(handler.get_last_event_time().await.is_none());
    }
} 
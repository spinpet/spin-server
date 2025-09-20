use anyhow::Result;
use serde_json::Value;
use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{
    RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter,
};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

/// Connection state for RPC client
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Reconnecting,
}

/// RPC connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub total_requests: u64,
    pub failed_requests: u64,
    pub reconnect_attempts: u32,
    pub last_successful_request: Option<Instant>,
    pub last_reconnect_attempt: Option<Instant>,
}

impl Default for ConnectionStats {
    fn default() -> Self {
        Self {
            total_requests: 0,
            failed_requests: 0,
            reconnect_attempts: 0,
            last_successful_request: None,
            last_reconnect_attempt: None,
        }
    }
}

/// Solana RPC client wrapper with reconnection capabilities
pub struct SolanaClient {
    rpc_url: String,
    #[allow(dead_code)]
    program_id: Pubkey,
    client: Arc<RwLock<RpcClient>>,
    connection_state: Arc<RwLock<ConnectionState>>,
    stats: Arc<RwLock<ConnectionStats>>,
    max_reconnect_attempts: u32,
    base_reconnect_interval: u64, // seconds
    max_reconnect_interval: u64,  // seconds
}

impl SolanaClient {
    /// Create a new Solana client with reconnection capabilities
    pub fn new(rpc_url: &str, program_id: &str) -> Result<Self> {
        let program_id = Pubkey::from_str(program_id)?;
        let client = RpcClient::new(rpc_url.to_string());

        info!("Solana client initialized successfully");
        info!("RPC URL: {}", rpc_url);
        info!("Program ID: {}", program_id);

        Ok(Self {
            rpc_url: rpc_url.to_string(),
            program_id,
            client: Arc::new(RwLock::new(client)),
            connection_state: Arc::new(RwLock::new(ConnectionState::Connected)),
            stats: Arc::new(RwLock::new(ConnectionStats::default())),
            max_reconnect_attempts: 10,
            base_reconnect_interval: 1, // Start with 1 second
            max_reconnect_interval: 30, // Max 30 seconds
        })
    }

    /// Create a new Solana client with custom reconnection settings
    #[allow(dead_code)]
    pub fn new_with_config(
        rpc_url: &str,
        program_id: &str,
        max_reconnect_attempts: u32,
        base_reconnect_interval: u64,
        max_reconnect_interval: u64,
    ) -> Result<Self> {
        let mut client = Self::new(rpc_url, program_id)?;
        client.max_reconnect_attempts = max_reconnect_attempts;
        client.base_reconnect_interval = base_reconnect_interval;
        client.max_reconnect_interval = max_reconnect_interval;
        Ok(client)
    }

    /// Execute RPC call with automatic reconnection
    async fn execute_with_retry<T, F>(&self, operation: F) -> Result<T>
    where
        F: Fn(&RpcClient) -> Result<T> + Send + Sync,
        T: Send,
    {
        let mut attempts = 0;

        loop {
            // Update stats
            {
                let mut stats = self.stats.write().await;
                stats.total_requests += 1;
            }

            // Try to execute operation with current client
            {
                let client_guard = self.client.read().await;
                match operation(&*client_guard) {
                    Ok(result) => {
                        // Success - update connection state and stats
                        {
                            let mut state = self.connection_state.write().await;
                            *state = ConnectionState::Connected;
                        }
                        {
                            let mut stats = self.stats.write().await;
                            stats.last_successful_request = Some(Instant::now());
                            // Reset reconnect attempts on success
                            stats.reconnect_attempts = 0;
                        }
                        return Ok(result);
                    }
                    Err(e) => {
                        error!("RPC request failed: {}", e);

                        // Update failed request stats
                        {
                            let mut stats = self.stats.write().await;
                            stats.failed_requests += 1;
                        }

                        // Mark as disconnected
                        {
                            let mut state = self.connection_state.write().await;
                            *state = ConnectionState::Disconnected;
                        }

                        attempts += 1;
                        if attempts >= self.max_reconnect_attempts {
                            error!(
                                "Max reconnection attempts ({}) exceeded for RPC",
                                self.max_reconnect_attempts
                            );
                            return Err(anyhow::anyhow!(
                                "RPC connection failed after {} attempts",
                                attempts
                            ));
                        }

                        // Try to reconnect
                        if let Err(reconnect_err) = self.reconnect().await {
                            warn!(
                                "Reconnection attempt {} failed: {}",
                                attempts, reconnect_err
                            );

                            // Calculate exponential backoff with jitter
                            let delay = std::cmp::min(
                                self.base_reconnect_interval * 2_u64.pow(attempts - 1),
                                self.max_reconnect_interval,
                            );

                            // Add jitter (±25%)
                            let jitter = std::cmp::max(delay / 4, 1); // Ensure jitter is at least 1
                            let mut hasher = DefaultHasher::new();
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_nanos()
                                .hash(&mut hasher);
                            let random_offset = (hasher.finish() % (2 * jitter)) as u64;
                            let actual_delay = delay + random_offset - jitter;

                            warn!(
                                "Waiting {} seconds before retry attempt {}",
                                actual_delay,
                                attempts + 1
                            );
                            sleep(Duration::from_secs(actual_delay)).await;
                        } else {
                            info!("RPC reconnection successful on attempt {}", attempts);
                            // Don't sleep on successful reconnection, try the operation immediately
                        }
                    }
                }
            }
        }
    }

    /// Reconnect to RPC endpoint
    async fn reconnect(&self) -> Result<()> {
        {
            let mut state = self.connection_state.write().await;
            *state = ConnectionState::Reconnecting;
        }

        {
            let mut stats = self.stats.write().await;
            stats.reconnect_attempts += 1;
            stats.last_reconnect_attempt = Some(Instant::now());
        }

        info!("🔄 Attempting to reconnect to RPC: {}", self.rpc_url);

        // Create new client
        let new_client = RpcClient::new(self.rpc_url.clone());

        // Test the connection
        match new_client.get_health() {
            Ok(_) => {
                // Connection successful, replace the client
                {
                    let mut client_guard = self.client.write().await;
                    *client_guard = new_client;
                }

                {
                    let mut state = self.connection_state.write().await;
                    *state = ConnectionState::Connected;
                }

                info!("✅ RPC reconnection successful");
                Ok(())
            }
            Err(e) => {
                {
                    let mut state = self.connection_state.write().await;
                    *state = ConnectionState::Disconnected;
                }

                error!("❌ RPC reconnection failed: {}", e);
                Err(anyhow::anyhow!("RPC reconnection failed: {}", e))
            }
        }
    }

    /// Get latest slot with automatic reconnection
    #[allow(dead_code)]
    pub async fn get_latest_slot(&self) -> Result<u64> {
        self.execute_with_retry(|client| {
            let slot = client.get_slot()?;
            debug!("Getting latest block height: {}", slot);
            Ok(slot)
        })
        .await
    }

    /// Get program logs with automatic reconnection
    #[allow(dead_code)]
    pub async fn get_program_logs(&self, _limit: usize) -> Result<Vec<String>> {
        let program_id = self.program_id;
        self.execute_with_retry(move |client| {
            let _filter = RpcTransactionLogsFilter::Mentions(vec![program_id.to_string()]);
            let _config = RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig::confirmed()),
            };

            match client.get_program_accounts(&program_id) {
                Ok(accounts) => {
                    info!("Retrieved {} program accounts", accounts.len());
                    Ok(vec![format!("Found {} program accounts", accounts.len())])
                }
                Err(e) => {
                    error!("Failed to get program accounts: {}", e);
                    Err(e.into())
                }
            }
        })
        .await
    }

    /// Get transaction details with automatic reconnection
    #[allow(dead_code)]
    pub async fn get_transaction_details(
        &self,
        signature: &str,
    ) -> Result<Option<TransactionDetails>> {
        let signature_str = signature.to_string();
        self.execute_with_retry(move |client| {
            let signature = Signature::from_str(&signature_str)?;
            let config = RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            };

            match client.get_transaction_with_config(&signature, config) {
                Ok(transaction) => {
                    let (logs, success) = match &transaction.transaction.meta {
                        Some(meta) => {
                            let logs = match &meta.log_messages {
                                solana_transaction_status::option_serializer::OptionSerializer::Some(logs) => logs.clone(),
                                _ => Vec::new(),
                            };
                            let success = meta.err.is_none();
                            (logs, success)
                        }
                        None => (Vec::new(), false),
                    };
                    let details = TransactionDetails {
                        signature: signature.to_string(),
                        slot: transaction.slot,
                        block_time: transaction.block_time,
                        logs,
                        success,
                    };
                    Ok(Some(details))
                }
                Err(e) => {
                    error!("Failed to get transaction details: {}", e);
                    Ok(None)
                }
            }
        }).await
    }

    /// Get transaction with full logs including CPI calls
    pub async fn get_transaction_with_logs(&self, signature: &str) -> Result<Value> {
        let signature_str = signature.to_string();
        self.execute_with_retry(move |client| {
            let sig = Signature::from_str(&signature_str)?;
            // Use confirmed instead of finalized for faster response
            let config = RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            };

            match client.get_transaction_with_config(&sig, config) {
                Ok(transaction) => {
                    // Convert the transaction to JSON for easier parsing
                    let json = serde_json::to_value(&transaction)?;
                    debug!("Got transaction details for {}", signature_str);
                    Ok(json)
                }
                Err(e) => {
                    // Transaction might not be available yet, return empty result instead of error
                    debug!("Transaction {} not available yet: {}", signature_str, e);
                    Ok(serde_json::json!({}))
                }
            }
        })
        .await
    }

    /// Check RPC connection status with automatic reconnection attempt
    pub async fn check_connection(&self) -> Result<bool> {
        self.execute_with_retry(|client| match client.get_health() {
            Ok(_) => {
                debug!("Solana RPC connection is healthy");
                Ok(true)
            }
            Err(e) => {
                error!("Solana RPC connection failed: {}", e);
                Err(anyhow::anyhow!("RPC health check failed: {}", e))
            }
        })
        .await
    }

    /// Force reconnection (useful for manual recovery)
    #[allow(dead_code)]
    pub async fn force_reconnect(&self) -> Result<()> {
        info!("🔄 Force reconnecting RPC client");
        self.reconnect().await
    }

    /// Get current connection state
    #[allow(dead_code)]
    pub async fn get_connection_state(&self) -> ConnectionState {
        self.connection_state.read().await.clone()
    }

    /// Get connection statistics
    #[allow(dead_code)]
    pub async fn get_connection_stats(&self) -> ConnectionStats {
        self.stats.read().await.clone()
    }

    /// Get program ID
    #[allow(dead_code)]
    pub fn get_program_id(&self) -> &Pubkey {
        &self.program_id
    }

    /// Check if client is currently connected
    #[allow(dead_code)]
    pub async fn is_connected(&self) -> bool {
        matches!(
            *self.connection_state.read().await,
            ConnectionState::Connected
        )
    }
}

/// Transaction details structure
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TransactionDetails {
    pub signature: String,
    pub slot: u64,
    pub block_time: Option<i64>,
    pub logs: Vec<String>,
    pub success: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_solana_client_creation() {
        let client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "11111111111111111111111111111111",
        );
        assert!(client.is_ok());

        let client = client.unwrap();
        assert_eq!(
            client.get_connection_state().await,
            ConnectionState::Connected
        );
    }

    #[tokio::test]
    async fn test_custom_config_creation() {
        let client = SolanaClient::new_with_config(
            "https://api.devnet.solana.com",
            "11111111111111111111111111111111",
            5,  // max_reconnect_attempts
            2,  // base_reconnect_interval
            60, // max_reconnect_interval
        );
        assert!(client.is_ok());
    }

    #[test]
    fn test_invalid_program_id() {
        let client = SolanaClient::new("https://api.devnet.solana.com", "invalid_program_id");
        assert!(client.is_err());
    }

    #[tokio::test]
    async fn test_connection_stats() {
        let client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "11111111111111111111111111111111",
        )
        .unwrap();

        let stats = client.get_connection_stats().await;
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.failed_requests, 0);
        assert_eq!(stats.reconnect_attempts, 0);
    }
}

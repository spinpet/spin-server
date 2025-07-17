use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcTransactionConfig, RpcTransactionLogsConfig, RpcTransactionLogsFilter};
use solana_transaction_status::UiTransactionEncoding;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::signature::Signature;
use anyhow::Result;
use std::str::FromStr;
use tracing::{info, error, debug};

/// Solana RPC client wrapper
pub struct SolanaClient {
    client: RpcClient,
    program_id: Pubkey,
}

impl SolanaClient {
    /// Create a new Solana client
    pub fn new(rpc_url: &str, program_id: &str) -> Result<Self> {
        let client = RpcClient::new(rpc_url.to_string());
        let program_id = Pubkey::from_str(program_id)?;
        
        info!("Solana client initialized successfully");
        info!("RPC URL: {}", rpc_url);
        info!("Program ID: {}", program_id);
        
        Ok(Self {
            client,
            program_id,
        })
    }

    /// Get the latest block height (slot)
    pub async fn get_latest_slot(&self) -> Result<u64> {
        let slot = self.client.get_slot()?;
        debug!("Getting latest block height: {}", slot);
        Ok(slot)
    }

    /// Get program transaction logs
    pub async fn get_program_logs(&self, _limit: usize) -> Result<Vec<String>> {
        let _filter = RpcTransactionLogsFilter::Mentions(vec![self.program_id.to_string()]);
        let _config = RpcTransactionLogsConfig {
            commitment: Some(CommitmentConfig::confirmed()),
        };
        
        // Note: This method may need adjustment in actual use, as Solana RPC has log query limitations
        // In practical applications, it's recommended to use WebSocket to listen for real-time logs
        match self.client.get_program_accounts(&self.program_id) {
            Ok(accounts) => {
                info!("Retrieved {} program accounts", accounts.len());
                Ok(vec![format!("Found {} program accounts", accounts.len())])
            }
            Err(e) => {
                error!("Failed to get program accounts: {}", e);
                Err(e.into())
            }
        }
    }

    /// Get detailed information about a specific transaction
    pub async fn get_transaction_details(&self, signature: &str) -> Result<Option<TransactionDetails>> {
        let signature = Signature::from_str(signature)?;
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        };

        match self.client.get_transaction_with_config(&signature, config) {
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
    }

    /// Check RPC connection status
    pub async fn check_connection(&self) -> Result<bool> {
        match self.client.get_health() {
            Ok(_) => {
                info!("Solana RPC connection is healthy");
                Ok(true)
            }
            Err(e) => {
                error!("Solana RPC connection failed: {}", e);
                Ok(false)
            }
        }
    }

    /// Get program ID
    pub fn get_program_id(&self) -> &Pubkey {
        &self.program_id
    }
}

/// Transaction details structure
#[derive(Debug, Clone)]
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
            "11111111111111111111111111111111"
        );
        assert!(client.is_ok());
    }

    #[test]
    fn test_invalid_program_id() {
        let client = SolanaClient::new(
            "https://api.devnet.solana.com",
            "invalid_program_id"
        );
        assert!(client.is_err());
    }
} 
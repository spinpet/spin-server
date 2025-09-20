use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)] 
pub struct Config {
    pub server: ServerConfig,
    pub cors: CorsConfig,
    pub logging: LoggingConfig,
    pub solana: SolanaConfig,
    pub database: DatabaseConfig,
    pub ipfs: IpfsConfig,
    pub kline: KlineServiceConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CorsConfig {
    pub enabled: bool,
    pub allow_origins: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    pub level: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SolanaConfig {
    pub rpc_url: String,
    pub ws_url: String,
    pub program_id: String,
    pub enable_event_listener: bool,
    pub commitment: String,
    #[allow(dead_code)]
    pub reconnect_interval: u64,
    #[allow(dead_code)]
    pub max_reconnect_attempts: u32,
    #[allow(dead_code)]
    pub event_buffer_size: usize,
    #[allow(dead_code)]
    pub event_batch_size: usize,
    #[allow(dead_code)]
    pub ping_interval_seconds: u64,
    /// Whether to process failed transactions for development/testing (default: false)
    #[serde(default)]
    pub process_failed_transactions: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub rocksdb_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IpfsConfig {
    pub gateway_url: String,
    pub request_timeout_seconds: u64,
    pub max_retries: u32,
    pub retry_delay_seconds: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KlineServiceConfig {
    pub enable_kline_service: bool,
    pub connection_timeout_secs: u64,
    pub max_subscriptions_per_client: usize,
    pub history_data_limit: usize,
    pub ping_interval_secs: u64,
    pub ping_timeout_secs: u64,
}

impl Config {
    pub fn new() -> anyhow::Result<Self> {
        let run_mode = env::var("RUST_ENV").unwrap_or_else(|_| "development".into());
        
        let mut builder = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(config::File::with_name(&format!("config/{}", run_mode)).required(false))
            .add_source(config::Environment::with_prefix("APP"));

        // If SERVER_PORT environment variable is set, override the configuration
        if let Ok(port) = env::var("SERVER_PORT") {
            if let Ok(port_num) = port.parse::<u16>() {
                builder = builder.set_override("server.port", port_num)?;
            }
        }

        let settings = builder.build()?;
        let config: Config = settings.try_deserialize()?;
        Ok(config)
    }
} 
use serde::Deserialize;
use std::env;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub cors: CorsConfig,
    pub logging: LoggingConfig,
    pub solana: SolanaConfig,
    pub database: DatabaseConfig,
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
    pub reconnect_interval: u64,
    pub max_reconnect_attempts: u32,
    pub event_buffer_size: usize,
    pub event_batch_size: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub rocksdb_path: String,
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
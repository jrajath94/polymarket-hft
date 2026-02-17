use polymarket_hft::{AppConfig, Result};
use std::env;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing_subscriber::filter::LevelFilter::INFO.into()),
        )
        .json()
        .init();

    // Load configuration
    let config_path = env::var("APP_CONFIG_PATH").unwrap_or_else(|_| "config/default.toml".to_string());
    let config = polymarket_hft::config::load_config(&config_path)?;

    tracing::info!(
        app = %config.app.name,
        environment = %config.app.environment,
        "🚀 Polymarket HFT Engine starting"
    );

    // TODO: Phases 2-8 will connect WS, spawn strategies, etc.
    tracing::info!("Phase 1 foundation ready. Awaiting Phase 2 implementation...");

    Ok(())
}

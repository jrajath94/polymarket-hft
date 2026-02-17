use crate::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Global application configuration loaded from TOML + env overrides.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppConfig {
    pub app: AppSection,
    pub api: ApiSection,
    pub ws: WsSection,
    pub fees: FeesSection,
    pub risk: RiskSection,
    pub rate_limits: RateLimitsSection,
    pub monitoring: MonitoringSection,
    pub strategies: StrategiesSection,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AppSection {
    pub name: String,
    pub environment: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ApiSection {
    pub clob_url: String,
    pub gamma_url: String,
    pub data_api_url: String,
    pub rtds_url: String,
    pub sports_url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WsSection {
    pub market_subscribe_url: String,
    pub user_subscribe_url: String,
    pub heartbeat_interval_secs: u64,
    pub reconnect_delay_ms: u64,
    pub max_reconnect_attempts: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeesSection {
    pub crypto_5m_taker: f64,
    pub crypto_15m_taker: f64,
    pub ncaab_taker: f64,
    pub serie_a_taker: f64,
    pub default_taker: f64,
    pub maker_rebate_pct: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RiskSection {
    pub max_daily_drawdown: f64,
    pub max_consecutive_losses: u32,
    pub ws_stale_threshold_secs: u64,
    pub kelly_fraction: f64,
    pub max_single_position_pct: f64,
    pub max_portfolio_leverage: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RateLimitsSection {
    pub orders_per_10s: u32,
    pub book_reads_per_10s: u32,
    pub general_per_10s: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitoringSection {
    pub prometheus_listen: String,
    pub health_check_interval_secs: u64,
    pub metrics_retention_mins: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StrategiesSection {
    pub spread_farming: SpreadFarmingConfig,
    pub weather: WeatherConfig,
    pub copy_trade: CopyTradeConfig,
    pub lp: LpConfig,
    pub penny_longshot: PennyLongshotConfig,
    pub custom_bot: CustomBotConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpreadFarmingConfig {
    pub enabled: bool,
    pub min_arb_pct: f64,
    pub max_order_size: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WeatherConfig {
    pub enabled: bool,
    pub forecast_days: u32,
    pub revalidate_gridpoint_hours: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CopyTradeConfig {
    pub enabled: bool,
    pub velocity_threshold_pct: f64,
    pub velocity_window_secs: u64,
    pub dedup_ttl_hours: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LpConfig {
    pub enabled: bool,
    pub post_only: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PennyLongshotConfig {
    pub enabled: bool,
    pub min_positions: u32,
    pub max_positions: u32,
    pub max_price: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CustomBotConfig {
    pub enabled: bool,
    pub rsi_period: u32,
    pub rsi_oversold: f64,
    pub rsi_overbought: f64,
}

/// Load configuration from TOML file at given path.
/// Env vars (prefixed with APP_) override TOML values.
pub fn load_config<P: AsRef<Path>>(config_path: P) -> Result<AppConfig> {
    let config_path = config_path.as_ref();

    // Load from TOML
    let settings = config::Config::builder()
        .add_source(config::File::from(config_path))
        .add_source(config::Environment::with_prefix("APP").try_parsing(true).separator("__"))
        .build()
        .map_err(|e| AppError::Config(format!("failed to load config: {}", e)))?;

    let config = settings
        .try_deserialize::<AppConfig>()
        .map_err(|e| AppError::Config(format!("failed to deserialize config: {}", e)))?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_structure_created() {
        // Verify that AppConfig can be instantiated.
        // This is a smoke test for the struct definition.
        let _config = AppConfig {
            app: AppSection {
                name: "test".to_string(),
                environment: "dev".to_string(),
            },
            api: ApiSection {
                clob_url: "https://example.com".to_string(),
                gamma_url: "https://gamma.example.com".to_string(),
                data_api_url: "https://data.example.com".to_string(),
                rtds_url: "wss://rtds.example.com".to_string(),
                sports_url: "wss://sports.example.com".to_string(),
            },
            ws: WsSection {
                market_subscribe_url: "wss://market.example.com".to_string(),
                user_subscribe_url: "wss://user.example.com".to_string(),
                heartbeat_interval_secs: 10,
                reconnect_delay_ms: 1000,
                max_reconnect_attempts: 10,
            },
            fees: FeesSection {
                crypto_5m_taker: 0.0156,
                crypto_15m_taker: 0.0156,
                ncaab_taker: 0.0156,
                serie_a_taker: 0.0156,
                default_taker: 0.0,
                maker_rebate_pct: 0.20,
            },
            risk: RiskSection {
                max_daily_drawdown: 0.10,
                max_consecutive_losses: 3,
                ws_stale_threshold_secs: 30,
                kelly_fraction: 0.25,
                max_single_position_pct: 0.05,
                max_portfolio_leverage: 2.0,
            },
            rate_limits: RateLimitsSection {
                orders_per_10s: 500,
                book_reads_per_10s: 1500,
                general_per_10s: 9000,
            },
            monitoring: MonitoringSection {
                prometheus_listen: "127.0.0.1:9090".to_string(),
                health_check_interval_secs: 5,
                metrics_retention_mins: 60,
            },
            strategies: StrategiesSection {
                spread_farming: SpreadFarmingConfig {
                    enabled: true,
                    min_arb_pct: 0.01,
                    max_order_size: 100.0,
                },
                weather: WeatherConfig {
                    enabled: true,
                    forecast_days: 5,
                    revalidate_gridpoint_hours: 24,
                },
                copy_trade: CopyTradeConfig {
                    enabled: true,
                    velocity_threshold_pct: 2.0,
                    velocity_window_secs: 10,
                    dedup_ttl_hours: 1,
                },
                lp: LpConfig {
                    enabled: false,
                    post_only: true,
                },
                penny_longshot: PennyLongshotConfig {
                    enabled: false,
                    min_positions: 20,
                    max_positions: 50,
                    max_price: 0.05,
                },
                custom_bot: CustomBotConfig {
                    enabled: false,
                    rsi_period: 14,
                    rsi_oversold: 30.0,
                    rsi_overbought: 70.0,
                },
            },
        };

        assert_eq!(_config.app.name, "test");
        assert_eq!(_config.fees.crypto_5m_taker, 0.0156);
        assert_eq!(_config.risk.kelly_fraction, 0.25);
    }

    #[test]
    fn test_load_config_from_file() {
        // This test requires the actual config file to exist.
        // For now, we verify the function signature works.
        let result = load_config("config/default.toml");
        assert!(result.is_ok(), "Failed to load config: {:?}", result.err());

        let config = result.unwrap();
        assert_eq!(config.app.name, "polymarket-hft");
        assert_eq!(config.fees.crypto_5m_taker, 0.0156);
        assert!(config.strategies.spread_farming.enabled);
    }

    #[test]
    fn test_rate_limits_config() {
        let config = AppConfig {
            app: AppSection {
                name: "test".to_string(),
                environment: "dev".to_string(),
            },
            api: ApiSection {
                clob_url: "https://example.com".to_string(),
                gamma_url: "https://gamma.example.com".to_string(),
                data_api_url: "https://data.example.com".to_string(),
                rtds_url: "wss://rtds.example.com".to_string(),
                sports_url: "wss://sports.example.com".to_string(),
            },
            ws: WsSection {
                market_subscribe_url: "wss://market.example.com".to_string(),
                user_subscribe_url: "wss://user.example.com".to_string(),
                heartbeat_interval_secs: 10,
                reconnect_delay_ms: 1000,
                max_reconnect_attempts: 10,
            },
            fees: FeesSection {
                crypto_5m_taker: 0.0156,
                crypto_15m_taker: 0.0156,
                ncaab_taker: 0.0156,
                serie_a_taker: 0.0156,
                default_taker: 0.0,
                maker_rebate_pct: 0.20,
            },
            risk: RiskSection {
                max_daily_drawdown: 0.10,
                max_consecutive_losses: 3,
                ws_stale_threshold_secs: 30,
                kelly_fraction: 0.25,
                max_single_position_pct: 0.05,
                max_portfolio_leverage: 2.0,
            },
            rate_limits: RateLimitsSection {
                orders_per_10s: 500,
                book_reads_per_10s: 1500,
                general_per_10s: 9000,
            },
            monitoring: MonitoringSection {
                prometheus_listen: "127.0.0.1:9090".to_string(),
                health_check_interval_secs: 5,
                metrics_retention_mins: 60,
            },
            strategies: StrategiesSection {
                spread_farming: SpreadFarmingConfig {
                    enabled: true,
                    min_arb_pct: 0.01,
                    max_order_size: 100.0,
                },
                weather: WeatherConfig {
                    enabled: true,
                    forecast_days: 5,
                    revalidate_gridpoint_hours: 24,
                },
                copy_trade: CopyTradeConfig {
                    enabled: true,
                    velocity_threshold_pct: 2.0,
                    velocity_window_secs: 10,
                    dedup_ttl_hours: 1,
                },
                lp: LpConfig {
                    enabled: false,
                    post_only: true,
                },
                penny_longshot: PennyLongshotConfig {
                    enabled: false,
                    min_positions: 20,
                    max_positions: 50,
                    max_price: 0.05,
                },
                custom_bot: CustomBotConfig {
                    enabled: false,
                    rsi_period: 14,
                    rsi_oversold: 30.0,
                    rsi_overbought: 70.0,
                },
            },
        };

        assert_eq!(config.rate_limits.orders_per_10s, 500);
        assert_eq!(config.rate_limits.book_reads_per_10s, 1500);
    }
}

// Polymarket HFT Trading Engine
//
// A production-grade Rust async trading system targeting prediction market arbitrage.
// All prices use Decimal (never f64). TDD throughout.

pub mod error;
pub mod config;

// Module declarations (will be implemented in phases)
pub mod auth {
    pub mod l2;
    pub mod eip712;
}

pub mod clob {
    pub mod types;
    pub mod ws_client;
    pub mod rest_client;
}

pub mod gamma {
    pub mod client;
}

pub mod data_api {
    pub mod client;
}

pub mod rtds {
    pub mod ws_client;
}

pub mod sports_ws {
    pub mod client;
}

pub mod noaa {
    pub mod client;
}

pub mod orderbook {
    pub mod cache;
}

pub mod executor {
    pub mod fee_calc;
    pub mod order_builder;
    pub mod batch_executor;
    pub mod rate_limiter;
}

pub mod risk {
    pub mod kelly;
    pub mod circuit_breaker;
    pub mod position_tracker;
}

pub mod strategies {
    pub mod spread_farming;
    pub mod weather;
    pub mod copy_trade;
    pub mod lp;
    pub mod penny_longshot;
    pub mod custom_bot;
}

pub mod monitoring {
    pub mod metrics;
    pub mod health;
}

pub mod backtest;

pub use config::AppConfig;
pub use error::{AppError, Result};

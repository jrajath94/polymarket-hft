use thiserror::Error;

/// Unified error type for the HFT engine.
#[derive(Error, Debug)]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("authentication error: {0}")]
    Auth(String),

    #[error("order building error: {0}")]
    OrderBuilder(String),

    #[error("rate limit exceeded: {0}")]
    RateLimit(String),

    #[error("websocket error: {0}")]
    WebSocket(String),

    #[error("order book stale: last update {0}ms ago")]
    OrderBookStale(u64),

    #[error("circuit breaker triggered: {0}")]
    CircuitBreaker(String),

    #[error("http error: {0}")]
    Http(String),

    #[error("invalid market: {0}")]
    InvalidMarket(String),

    #[error("position limit exceeded: {0}")]
    PositionLimit(String),

    #[error("decimal conversion error: {0}")]
    DecimalConversion(String),

    #[error("unknown error: {0}")]
    Unknown(String),
}

/// Result type alias for AppError.
pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = AppError::Config("invalid api_url".to_string());
        assert!(err.to_string().contains("configuration error"));
        assert!(err.to_string().contains("invalid api_url"));
    }

    #[test]
    fn test_error_debug() {
        let err = AppError::RateLimit("orders exceeded".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("RateLimit"));
    }

    #[test]
    fn test_result_type() {
        let ok_result: Result<i32> = Ok(42);
        assert_eq!(ok_result.unwrap(), 42);

        let err_result: Result<i32> = Err(AppError::Unknown("test".to_string()));
        assert!(err_result.is_err());
    }

    #[test]
    fn test_circuit_breaker_error() {
        let err = AppError::CircuitBreaker("daily drawdown exceeded".to_string());
        assert!(err.to_string().contains("circuit breaker triggered"));
    }
}

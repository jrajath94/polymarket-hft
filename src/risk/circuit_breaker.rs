// Circuit breaker — hard stops for risk management.
//
// Rules:
// 1. Drawdown > 10% of starting capital: halt ALL trading
// 2. Consecutive losses > 3 for a strategy: halt that strategy
// 3. WebSocket stale > 30s: halt strategies dependent on that feed
//
// The circuit breaker is checked before every order placement.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::RwLock;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;

/// Reason a circuit breaker was tripped.
#[derive(Debug, Clone, PartialEq)]
pub enum TripReason {
    MaxDrawdown { current: Decimal, threshold: Decimal },
    ConsecutiveLosses { strategy: String, count: u32, max: u32 },
    WsStale { feed: String, stale_secs: u64, threshold_secs: u64 },
}

/// Configuration for the circuit breaker.
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    /// Max drawdown as fraction (e.g., 0.10 = 10%)
    pub max_drawdown: Decimal,
    /// Max consecutive losses per strategy before halting it
    pub max_consecutive_losses: u32,
    /// WebSocket staleness threshold in seconds
    pub ws_stale_threshold_secs: u64,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            max_drawdown: dec!(0.10),
            max_consecutive_losses: 3,
            ws_stale_threshold_secs: 30,
        }
    }
}

/// Per-strategy loss tracking state.
#[derive(Debug, Clone)]
struct StrategyState {
    consecutive_losses: u32,
    halted: bool,
}

/// Circuit breaker that gates order execution.
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    starting_capital: Decimal,
    current_capital: Arc<RwLock<Decimal>>,
    global_halt: Arc<RwLock<bool>>,
    strategy_states: Arc<DashMap<String, StrategyState>>,
    ws_last_heartbeat: Arc<DashMap<String, DateTime<Utc>>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig, starting_capital: Decimal) -> Self {
        Self {
            config,
            starting_capital,
            current_capital: Arc::new(RwLock::new(starting_capital)),
            global_halt: Arc::new(RwLock::new(false)),
            strategy_states: Arc::new(DashMap::new()),
            ws_last_heartbeat: Arc::new(DashMap::new()),
        }
    }

    /// Check if a strategy is allowed to place orders.
    /// Returns Ok(()) if allowed, Err(TripReason) if blocked.
    pub fn check(&self, strategy: &str, now: DateTime<Utc>) -> Result<(), TripReason> {
        // 1. Global halt (drawdown breaker)
        if *self.global_halt.read() {
            let drawdown = self.current_drawdown();
            return Err(TripReason::MaxDrawdown {
                current: drawdown,
                threshold: self.config.max_drawdown,
            });
        }

        // 2. Check drawdown in real time
        let drawdown = self.current_drawdown();
        if drawdown > self.config.max_drawdown {
            *self.global_halt.write() = true;
            return Err(TripReason::MaxDrawdown {
                current: drawdown,
                threshold: self.config.max_drawdown,
            });
        }

        // 3. Per-strategy consecutive loss halt
        if let Some(state) = self.strategy_states.get(strategy) {
            if state.halted {
                return Err(TripReason::ConsecutiveLosses {
                    strategy: strategy.to_string(),
                    count: state.consecutive_losses,
                    max: self.config.max_consecutive_losses,
                });
            }
        }

        // 4. WebSocket staleness for dependent feeds
        for entry in self.ws_last_heartbeat.iter() {
            let feed = entry.key();
            let last_hb = entry.value();
            let stale_secs = (now - *last_hb).num_seconds().max(0) as u64;
            if stale_secs > self.config.ws_stale_threshold_secs {
                return Err(TripReason::WsStale {
                    feed: feed.clone(),
                    stale_secs,
                    threshold_secs: self.config.ws_stale_threshold_secs,
                });
            }
        }

        Ok(())
    }

    /// Record a trade result for a strategy.
    pub fn record_trade(&self, strategy: &str, is_win: bool) {
        let mut entry = self
            .strategy_states
            .entry(strategy.to_string())
            .or_insert(StrategyState {
                consecutive_losses: 0,
                halted: false,
            });

        if is_win {
            entry.consecutive_losses = 0;
        } else {
            entry.consecutive_losses += 1;
            if entry.consecutive_losses >= self.config.max_consecutive_losses {
                entry.halted = true;
            }
        }
    }

    /// Update the current capital (e.g., after a fill).
    pub fn update_capital(&self, new_capital: Decimal) {
        *self.current_capital.write() = new_capital;
    }

    /// Record a WebSocket heartbeat for a feed.
    pub fn record_ws_heartbeat(&self, feed: &str, at: DateTime<Utc>) {
        self.ws_last_heartbeat.insert(feed.to_string(), at);
    }

    /// Reset a halted strategy (manual intervention).
    pub fn reset_strategy(&self, strategy: &str) {
        if let Some(mut state) = self.strategy_states.get_mut(strategy) {
            state.consecutive_losses = 0;
            state.halted = false;
        }
    }

    /// Reset the global halt (manual intervention after capital injection).
    pub fn reset_global_halt(&self) {
        *self.global_halt.write() = false;
    }

    fn current_drawdown(&self) -> Decimal {
        if self.starting_capital.is_zero() {
            return dec!(0);
        }
        let current = *self.current_capital.read();
        let drawdown = (self.starting_capital - current) / self.starting_capital;
        drawdown.max(dec!(0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn base_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 2, 16, 12, 0, 0).unwrap()
    }

    fn default_breaker() -> CircuitBreaker {
        CircuitBreaker::new(CircuitBreakerConfig::default(), dec!(10000))
    }

    #[test]
    fn test_allows_trading_when_healthy() {
        let breaker = default_breaker();
        breaker.record_ws_heartbeat("clob", base_time());
        let result = breaker.check("spread", base_time());
        assert!(result.is_ok());
    }

    #[test]
    fn test_drawdown_halts_all_trading() {
        let breaker = default_breaker();
        // Lose 15% of capital (>10% threshold)
        breaker.update_capital(dec!(8500));

        let result = breaker.check("spread", base_time());
        assert!(result.is_err());
        match result.unwrap_err() {
            TripReason::MaxDrawdown { current, threshold } => {
                assert_eq!(current, dec!(0.15));
                assert_eq!(threshold, dec!(0.10));
            }
            other => panic!("expected MaxDrawdown, got {:?}", other),
        }
    }

    #[test]
    fn test_consecutive_losses_halt_strategy() {
        let breaker = default_breaker();

        // Record 3 consecutive losses for "copy" strategy
        breaker.record_trade("copy", false);
        breaker.record_trade("copy", false);
        breaker.record_trade("copy", false);

        // "copy" should be halted
        let result = breaker.check("copy", base_time());
        assert!(result.is_err());
        match result.unwrap_err() {
            TripReason::ConsecutiveLosses {
                strategy,
                count,
                max,
            } => {
                assert_eq!(strategy, "copy");
                assert_eq!(count, 3);
                assert_eq!(max, 3);
            }
            other => panic!("expected ConsecutiveLosses, got {:?}", other),
        }

        // Other strategies should still work
        let result2 = breaker.check("spread", base_time());
        assert!(result2.is_ok());
    }

    #[test]
    fn test_win_resets_consecutive_losses() {
        let breaker = default_breaker();

        breaker.record_trade("copy", false);
        breaker.record_trade("copy", false);
        // A win resets the counter
        breaker.record_trade("copy", true);
        breaker.record_trade("copy", false);

        // Only 1 loss after the win, should still be allowed
        let result = breaker.check("copy", base_time());
        assert!(result.is_ok());
    }

    #[test]
    fn test_ws_stale_halts_trading() {
        let breaker = default_breaker();
        // Last heartbeat was 45 seconds ago (>30s threshold)
        let stale_time = base_time() - chrono::Duration::seconds(45);
        breaker.record_ws_heartbeat("clob", stale_time);

        let result = breaker.check("spread", base_time());
        assert!(result.is_err());
        match result.unwrap_err() {
            TripReason::WsStale {
                feed,
                stale_secs,
                threshold_secs,
            } => {
                assert_eq!(feed, "clob");
                assert!(stale_secs >= 45);
                assert_eq!(threshold_secs, 30);
            }
            other => panic!("expected WsStale, got {:?}", other),
        }
    }

    #[test]
    fn test_fresh_heartbeat_allows_trading() {
        let breaker = default_breaker();
        // Heartbeat 5 seconds ago (<30s threshold)
        let fresh_time = base_time() - chrono::Duration::seconds(5);
        breaker.record_ws_heartbeat("clob", fresh_time);

        let result = breaker.check("spread", base_time());
        assert!(result.is_ok());
    }

    #[test]
    fn test_reset_strategy() {
        let breaker = default_breaker();

        // Halt copy strategy
        breaker.record_trade("copy", false);
        breaker.record_trade("copy", false);
        breaker.record_trade("copy", false);
        assert!(breaker.check("copy", base_time()).is_err());

        // Manual reset
        breaker.reset_strategy("copy");
        assert!(breaker.check("copy", base_time()).is_ok());
    }

    #[test]
    fn test_drawdown_at_boundary() {
        let breaker = default_breaker();
        // Exactly 10% drawdown: should trip (> not >=, but we use >)
        breaker.update_capital(dec!(9000));
        let result = breaker.check("spread", base_time());
        // 10% drawdown exactly should not trip (we use > not >=)
        assert!(result.is_ok());

        // 10.01% should trip
        breaker.update_capital(dec!(8999));
        let result = breaker.check("spread", base_time());
        assert!(result.is_err());
    }
}

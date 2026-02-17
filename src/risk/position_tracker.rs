// Position tracker — per-market exposure tracking with limits.
//
// Tracks:
// - Per-market position size
// - Total portfolio exposure
// - Enforces per-market and total leverage limits

use dashmap::DashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;

/// Configuration for position limits.
#[derive(Debug, Clone)]
pub struct PositionLimitsConfig {
    /// Max position size per market in USDC
    pub max_per_market: Decimal,
    /// Max total portfolio exposure in USDC
    pub max_total_exposure: Decimal,
    /// Max leverage ratio (total_exposure / capital)
    pub max_leverage: Decimal,
}

impl Default for PositionLimitsConfig {
    fn default() -> Self {
        Self {
            max_per_market: dec!(500),
            max_total_exposure: dec!(5000),
            max_leverage: dec!(2.0),
        }
    }
}

/// A position in a single market.
#[derive(Debug, Clone)]
pub struct Position {
    pub market_id: String,
    pub asset_id: String,
    pub size: Decimal,
    pub avg_entry_price: Decimal,
    pub current_price: Decimal,
}

impl Position {
    pub fn unrealized_pnl(&self) -> Decimal {
        (self.current_price - self.avg_entry_price) * self.size
    }

    pub fn notional_value(&self) -> Decimal {
        self.current_price * self.size
    }
}

/// Tracks all open positions and enforces limits.
pub struct PositionTracker {
    config: PositionLimitsConfig,
    capital: Decimal,
    positions: Arc<DashMap<String, Position>>,
}

impl PositionTracker {
    pub fn new(config: PositionLimitsConfig, capital: Decimal) -> Self {
        Self {
            config,
            capital,
            positions: Arc::new(DashMap::new()),
        }
    }

    /// Check if a new order of `size` USDC would exceed limits for a market.
    /// Returns Ok(()) if allowed, Err with reason string if blocked.
    pub fn check_order(&self, market_id: &str, additional_size: Decimal) -> Result<(), String> {
        // Per-market limit
        let current_market_size = self
            .positions
            .get(market_id)
            .map(|p| p.size)
            .unwrap_or(dec!(0));

        let new_market_size = current_market_size + additional_size;
        if new_market_size > self.config.max_per_market {
            return Err(format!(
                "per-market limit exceeded: {} + {} > {}",
                current_market_size, additional_size, self.config.max_per_market
            ));
        }

        // Total exposure limit
        let total_exposure = self.total_exposure() + additional_size;
        if total_exposure > self.config.max_total_exposure {
            return Err(format!(
                "total exposure limit exceeded: {} > {}",
                total_exposure, self.config.max_total_exposure
            ));
        }

        // Leverage check
        if self.capital > dec!(0) {
            let leverage = total_exposure / self.capital;
            if leverage > self.config.max_leverage {
                return Err(format!(
                    "leverage limit exceeded: {} > {}",
                    leverage, self.config.max_leverage
                ));
            }
        }

        Ok(())
    }

    /// Record a new or updated position.
    pub fn update_position(
        &self,
        market_id: &str,
        asset_id: &str,
        size: Decimal,
        avg_entry_price: Decimal,
        current_price: Decimal,
    ) {
        self.positions.insert(
            market_id.to_string(),
            Position {
                market_id: market_id.to_string(),
                asset_id: asset_id.to_string(),
                size,
                avg_entry_price,
                current_price,
            },
        );
    }

    /// Remove a closed position.
    pub fn close_position(&self, market_id: &str) {
        self.positions.remove(market_id);
    }

    /// Update the current price for a position (from market data).
    pub fn update_price(&self, market_id: &str, new_price: Decimal) {
        if let Some(mut pos) = self.positions.get_mut(market_id) {
            pos.current_price = new_price;
        }
    }

    /// Total exposure across all positions (sum of notional values).
    pub fn total_exposure(&self) -> Decimal {
        self.positions
            .iter()
            .map(|entry| entry.value().notional_value())
            .sum()
    }

    /// Total unrealized PnL across all positions.
    pub fn total_unrealized_pnl(&self) -> Decimal {
        self.positions
            .iter()
            .map(|entry| entry.value().unrealized_pnl())
            .sum()
    }

    /// Number of open positions.
    pub fn position_count(&self) -> usize {
        self.positions.len()
    }

    /// Get a snapshot of all positions.
    pub fn snapshot(&self) -> Vec<Position> {
        self.positions.iter().map(|e| e.value().clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_tracker() -> PositionTracker {
        PositionTracker::new(PositionLimitsConfig::default(), dec!(10000))
    }

    #[test]
    fn test_allows_order_within_limits() {
        let tracker = default_tracker();
        let result = tracker.check_order("market_1", dec!(100));
        assert!(result.is_ok());
    }

    #[test]
    fn test_blocks_per_market_limit() {
        let tracker = default_tracker();
        tracker.update_position("market_1", "0xabc", dec!(400), dec!(0.50), dec!(0.50));

        // Adding 200 would exceed 500 per-market limit
        let result = tracker.check_order("market_1", dec!(200));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("per-market limit"));
    }

    #[test]
    fn test_blocks_total_exposure_limit() {
        let tracker = default_tracker();
        // Fill up exposure close to 5000 limit
        tracker.update_position("m1", "0xa", dec!(2000), dec!(0.50), dec!(0.50));
        tracker.update_position("m2", "0xb", dec!(2000), dec!(0.50), dec!(0.50));
        // Current total: 2000*0.5 + 2000*0.5 = 2000. Still under.
        // Actually notional = size * current_price. Let me recalculate:
        // Position m1: 2000 shares @ 0.50 = 1000 notional
        // Position m2: 2000 shares @ 0.50 = 1000 notional
        // Total = 2000. Adding 3100 would give 5100 > 5000 limit.
        let result = tracker.check_order("m3", dec!(3100));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("total exposure"));
    }

    #[test]
    fn test_blocks_leverage_limit() {
        // Small capital, high exposure
        let tracker = PositionTracker::new(
            PositionLimitsConfig {
                max_per_market: dec!(10000),
                max_total_exposure: dec!(100000),
                max_leverage: dec!(2.0),
            },
            dec!(1000), // Only 1000 capital
        );

        // Trying to add 2100 would give leverage of 2.1 > 2.0
        let result = tracker.check_order("m1", dec!(2100));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("leverage"));
    }

    #[test]
    fn test_position_pnl() {
        let pos = Position {
            market_id: "m1".to_string(),
            asset_id: "0xabc".to_string(),
            size: dec!(100),
            avg_entry_price: dec!(0.50),
            current_price: dec!(0.65),
        };

        // PnL = (0.65 - 0.50) * 100 = 15
        assert_eq!(pos.unrealized_pnl(), dec!(15));
        // Notional = 0.65 * 100 = 65
        assert_eq!(pos.notional_value(), dec!(65));
    }

    #[test]
    fn test_total_unrealized_pnl() {
        let tracker = default_tracker();
        tracker.update_position("m1", "0xa", dec!(100), dec!(0.50), dec!(0.60));
        tracker.update_position("m2", "0xb", dec!(200), dec!(0.40), dec!(0.35));

        // m1 PnL: (0.60 - 0.50) * 100 = 10
        // m2 PnL: (0.35 - 0.40) * 200 = -10
        // Total = 0
        assert_eq!(tracker.total_unrealized_pnl(), dec!(0));
    }

    #[test]
    fn test_close_position() {
        let tracker = default_tracker();
        tracker.update_position("m1", "0xa", dec!(100), dec!(0.50), dec!(0.50));
        assert_eq!(tracker.position_count(), 1);

        tracker.close_position("m1");
        assert_eq!(tracker.position_count(), 0);
    }

    #[test]
    fn test_update_price() {
        let tracker = default_tracker();
        tracker.update_position("m1", "0xa", dec!(100), dec!(0.50), dec!(0.50));

        tracker.update_price("m1", dec!(0.75));

        let positions = tracker.snapshot();
        assert_eq!(positions[0].current_price, dec!(0.75));
        assert_eq!(positions[0].unrealized_pnl(), dec!(25));
    }
}

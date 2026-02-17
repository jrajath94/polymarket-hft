// Copy trading strategy — mirror whale trades with velocity filter and dedup.
//
// Key rules:
// - Velocity filter: skip if price moved >2% within 10s of whale trade (crowded)
// - Dedup: track trade_id in DashMap with 1h TTL, never process same trade twice
// - Mirror within 5s of detection; slippage check before execution

use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;

use crate::data_api::client::{TradeEvent, TradeSide};

/// Signal emitted by copy trade strategy for the executor to act on.
#[derive(Debug, Clone, PartialEq)]
pub struct CopySignal {
    pub asset_id: String,
    pub side: TradeSide,
    pub size: Decimal,
    pub max_price: Decimal,
    pub source_trade_id: String,
}

/// Price snapshot used for velocity checks.
#[derive(Debug, Clone)]
pub struct PriceSnapshot {
    pub price: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// Configuration for the copy trade strategy.
#[derive(Debug, Clone)]
pub struct CopyTradeConfig {
    /// Max % price move to tolerate (e.g., 2.0 means 2%)
    pub velocity_threshold_pct: Decimal,
    /// Window in seconds to check velocity
    pub velocity_window_secs: i64,
    /// TTL for dedup entries in hours
    pub dedup_ttl_hours: i64,
    /// Max slippage allowed on entry (e.g., 0.05 = 5%)
    pub max_slippage_pct: Decimal,
    /// Min trade size to mirror (filter noise)
    pub min_trade_size: Decimal,
    /// Price range filter: only mirror trades at this price or above
    pub min_price: Decimal,
    /// Price range filter: only mirror trades at this price or below
    pub max_price: Decimal,
}

impl Default for CopyTradeConfig {
    fn default() -> Self {
        Self {
            velocity_threshold_pct: dec!(2.0),
            velocity_window_secs: 10,
            dedup_ttl_hours: 1,
            max_slippage_pct: dec!(0.05),
            min_trade_size: dec!(5.0),
            min_price: dec!(0.30),
            max_price: dec!(0.90),
        }
    }
}

/// Entry in the dedup map: stores insertion time for TTL expiry.
#[derive(Debug, Clone)]
struct DedupEntry {
    inserted_at: DateTime<Utc>,
}

/// Copy trade strategy engine.
pub struct CopyTradeStrategy {
    config: CopyTradeConfig,
    /// trade_id -> DedupEntry for deduplication
    seen_trades: Arc<DashMap<String, DedupEntry>>,
}

impl CopyTradeStrategy {
    pub fn new(config: CopyTradeConfig) -> Self {
        Self {
            config,
            seen_trades: Arc::new(DashMap::new()),
        }
    }

    /// Evaluate a whale trade and decide whether to emit a copy signal.
    ///
    /// Returns None if:
    /// - Trade was already processed (dedup)
    /// - Price moved too fast (velocity filter)
    /// - Trade doesn't pass size/price filters
    pub fn evaluate(
        &self,
        trade: &TradeEvent,
        current_price: &PriceSnapshot,
        now: DateTime<Utc>,
    ) -> Option<CopySignal> {
        // 1. Size filter
        if trade.size < self.config.min_trade_size {
            return None;
        }

        // 2. Price range filter
        if trade.price < self.config.min_price || trade.price > self.config.max_price {
            return None;
        }

        // 3. Dedup check
        if self.is_duplicate(&trade.id, now) {
            return None;
        }

        // 4. Velocity filter: if price moved >threshold% within window, skip
        if self.is_crowded(trade, current_price) {
            return None;
        }

        // 5. Mark as seen
        self.seen_trades.insert(
            trade.id.clone(),
            DedupEntry {
                inserted_at: now,
            },
        );

        // 6. Compute max acceptable price with slippage
        let slippage = trade.price * self.config.max_slippage_pct;
        let max_price = match trade.side {
            TradeSide::Buy => trade.price + slippage,
            TradeSide::Sell => trade.price - slippage,
        };

        Some(CopySignal {
            asset_id: trade.asset_id.clone(),
            side: trade.side,
            size: trade.size,
            max_price,
            source_trade_id: trade.id.clone(),
        })
    }

    /// Check if a trade_id was already processed within the TTL.
    fn is_duplicate(&self, trade_id: &str, now: DateTime<Utc>) -> bool {
        if let Some(entry) = self.seen_trades.get(trade_id) {
            let age = now - entry.inserted_at;
            let ttl = Duration::hours(self.config.dedup_ttl_hours);
            if age < ttl {
                return true;
            }
            // Expired — remove and allow re-processing
            drop(entry);
            self.seen_trades.remove(trade_id);
        }
        false
    }

    /// Velocity filter: if price moved more than threshold since the trade, the market is crowded.
    fn is_crowded(&self, trade: &TradeEvent, current_price: &PriceSnapshot) -> bool {
        let time_diff = current_price.timestamp - trade.timestamp;
        if time_diff.num_seconds() > self.config.velocity_window_secs {
            return false; // Outside window, velocity check not applicable
        }

        if trade.price.is_zero() {
            return false;
        }

        let price_change = ((current_price.price - trade.price) / trade.price).abs();
        let threshold = self.config.velocity_threshold_pct / dec!(100);
        price_change > threshold
    }

    /// Purge expired entries from the dedup map. Call periodically.
    pub fn purge_expired(&self, now: DateTime<Utc>) {
        let ttl = Duration::hours(self.config.dedup_ttl_hours);
        self.seen_trades.retain(|_, entry| {
            now - entry.inserted_at < ttl
        });
    }

    /// Number of entries in the dedup map (for monitoring).
    pub fn dedup_map_size(&self) -> usize {
        self.seen_trades.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn base_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 2, 16, 12, 0, 0).unwrap()
    }

    fn make_trade(id: &str, price: Decimal, size: Decimal, side: TradeSide) -> TradeEvent {
        TradeEvent {
            id: id.to_string(),
            asset_id: "0xabc123".to_string(),
            market: "Test Market".to_string(),
            side,
            size,
            price,
            timestamp: base_time(),
            user: "0xWhale".to_string(),
        }
    }

    fn make_snapshot(price: Decimal, secs_after: i64) -> PriceSnapshot {
        PriceSnapshot {
            price,
            timestamp: base_time() + Duration::seconds(secs_after),
        }
    }

    #[test]
    fn test_basic_copy_signal_emitted() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t1", dec!(0.65), dec!(50.0), TradeSide::Buy);
        let snapshot = make_snapshot(dec!(0.65), 3); // 3s later, same price

        let signal = strategy.evaluate(&trade, &snapshot, base_time() + Duration::seconds(3));
        assert!(signal.is_some());

        let sig = signal.unwrap();
        assert_eq!(sig.asset_id, "0xabc123");
        assert_eq!(sig.side, TradeSide::Buy);
        assert_eq!(sig.size, dec!(50.0));
        assert_eq!(sig.source_trade_id, "t1");
        // max_price = 0.65 + (0.65 * 0.05) = 0.65 + 0.0325 = 0.6825
        assert_eq!(sig.max_price, dec!(0.6825));
    }

    #[test]
    fn test_velocity_filter_blocks_crowded_trades() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t2", dec!(0.50), dec!(100.0), TradeSide::Buy);
        // Price jumped 3% in 5 seconds (>2% threshold)
        let snapshot = make_snapshot(dec!(0.516), 5);

        let signal = strategy.evaluate(&trade, &snapshot, base_time() + Duration::seconds(5));
        assert!(signal.is_none(), "velocity filter should block crowded trade");
    }

    #[test]
    fn test_velocity_filter_allows_small_moves() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t3", dec!(0.50), dec!(100.0), TradeSide::Buy);
        // Price moved only 1% in 5 seconds (<2% threshold)
        let snapshot = make_snapshot(dec!(0.505), 5);

        let signal = strategy.evaluate(&trade, &snapshot, base_time() + Duration::seconds(5));
        assert!(signal.is_some(), "small price move should pass velocity filter");
    }

    #[test]
    fn test_velocity_outside_window_passes() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t4", dec!(0.50), dec!(100.0), TradeSide::Buy);
        // 15 seconds later (outside 10s window), big move
        let snapshot = make_snapshot(dec!(0.60), 15);

        let signal = strategy.evaluate(&trade, &snapshot, base_time() + Duration::seconds(15));
        assert!(signal.is_some(), "velocity outside window should pass");
    }

    #[test]
    fn test_dedup_blocks_duplicate_trade() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t5", dec!(0.65), dec!(50.0), TradeSide::Buy);
        let snapshot = make_snapshot(dec!(0.65), 3);
        let now = base_time() + Duration::seconds(3);

        // First call: should produce signal
        let sig1 = strategy.evaluate(&trade, &snapshot, now);
        assert!(sig1.is_some());

        // Second call with same trade_id: dedup blocks it
        let sig2 = strategy.evaluate(&trade, &snapshot, now);
        assert!(sig2.is_none(), "duplicate trade should be blocked");
    }

    #[test]
    fn test_dedup_expires_after_ttl() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t6", dec!(0.65), dec!(50.0), TradeSide::Buy);
        let snapshot = make_snapshot(dec!(0.65), 3);
        let now = base_time() + Duration::seconds(3);

        // First evaluation
        let sig1 = strategy.evaluate(&trade, &snapshot, now);
        assert!(sig1.is_some());

        // 2 hours later (beyond 1h TTL): should allow again
        let later = now + Duration::hours(2);
        let snapshot_later = PriceSnapshot {
            price: dec!(0.65),
            timestamp: later,
        };
        let sig2 = strategy.evaluate(&trade, &snapshot_later, later);
        assert!(sig2.is_some(), "expired dedup entry should allow re-processing");
    }

    #[test]
    fn test_min_size_filter() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t7", dec!(0.65), dec!(3.0), TradeSide::Buy); // $3 < $5 min
        let snapshot = make_snapshot(dec!(0.65), 3);

        let signal = strategy.evaluate(&trade, &snapshot, base_time() + Duration::seconds(3));
        assert!(signal.is_none(), "trade below min size should be filtered");
    }

    #[test]
    fn test_price_range_filter_too_low() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t8", dec!(0.10), dec!(50.0), TradeSide::Buy); // 10c < 30c min
        let snapshot = make_snapshot(dec!(0.10), 3);

        let signal = strategy.evaluate(&trade, &snapshot, base_time() + Duration::seconds(3));
        assert!(signal.is_none(), "price below range should be filtered");
    }

    #[test]
    fn test_price_range_filter_too_high() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t9", dec!(0.95), dec!(50.0), TradeSide::Buy); // 95c > 90c max
        let snapshot = make_snapshot(dec!(0.95), 3);

        let signal = strategy.evaluate(&trade, &snapshot, base_time() + Duration::seconds(3));
        assert!(signal.is_none(), "price above range should be filtered");
    }

    #[test]
    fn test_sell_side_slippage() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t10", dec!(0.60), dec!(50.0), TradeSide::Sell);
        let snapshot = make_snapshot(dec!(0.60), 3);

        let signal = strategy.evaluate(&trade, &snapshot, base_time() + Duration::seconds(3));
        let sig = signal.unwrap();
        // max_price for sell = 0.60 - (0.60 * 0.05) = 0.60 - 0.03 = 0.57
        assert_eq!(sig.max_price, dec!(0.57));
    }

    #[test]
    fn test_purge_expired_entries() {
        let strategy = CopyTradeStrategy::new(CopyTradeConfig::default());
        let trade = make_trade("t11", dec!(0.65), dec!(50.0), TradeSide::Buy);
        let snapshot = make_snapshot(dec!(0.65), 3);
        let now = base_time() + Duration::seconds(3);

        strategy.evaluate(&trade, &snapshot, now);
        assert_eq!(strategy.dedup_map_size(), 1);

        // Purge at now: entry is fresh, should remain
        strategy.purge_expired(now);
        assert_eq!(strategy.dedup_map_size(), 1);

        // Purge 2 hours later: should be removed
        strategy.purge_expired(now + Duration::hours(2));
        assert_eq!(strategy.dedup_map_size(), 0);
    }
}

use crate::backtest::engine::{BacktestFill, BacktestStrategy, BookSnapshot, TradeEvent};
use chrono::NaiveDateTime;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::{HashMap, VecDeque};

/// Copy trade slippage backtest: mirrors whale trades from the Data API trade stream.
/// Measures how much slippage occurs between the whale's fill price and our simulated fill.
pub struct CopyTradeSlippage {
    /// Tracked whale addresses
    whale_addresses: Vec<String>,
    /// Recent whale trades (for velocity detection)
    recent_trades: VecDeque<(NaiveDateTime, String, Decimal)>,
    /// Velocity window in seconds
    velocity_window_secs: i64,
    /// Min velocity (total size in window) to trigger copy
    velocity_threshold: Decimal,
    /// Assumed slippage basis points from whale fill to our fill
    slippage_bps: Decimal,
    taker_fee_pct: Decimal,
    max_order_size: Decimal,
    /// Dedup: market_id -> last trade timestamp
    dedup_map: HashMap<String, NaiveDateTime>,
    dedup_ttl_secs: i64,
}

impl CopyTradeSlippage {
    pub fn new(
        whale_addresses: Vec<String>,
        velocity_window_secs: i64,
        velocity_threshold: Decimal,
        slippage_bps: Decimal,
        taker_fee_pct: Decimal,
        max_order_size: Decimal,
        dedup_ttl_secs: i64,
    ) -> Self {
        Self {
            whale_addresses,
            recent_trades: VecDeque::new(),
            velocity_window_secs,
            velocity_threshold,
            slippage_bps,
            taker_fee_pct,
            max_order_size,
            dedup_map: HashMap::new(),
            dedup_ttl_secs,
        }
    }

    fn is_whale(&self, maker: &str) -> bool {
        self.whale_addresses.iter().any(|w| w == maker)
    }

    fn apply_slippage(&self, price: Decimal) -> Decimal {
        // Slippage increases our cost (buying higher, selling lower)
        price + (price * self.slippage_bps / dec!(10000))
    }

    fn prune_old_trades(&mut self, now: NaiveDateTime) {
        while let Some((ts, _, _)) = self.recent_trades.front() {
            if (now - *ts).num_seconds() > self.velocity_window_secs {
                self.recent_trades.pop_front();
            } else {
                break;
            }
        }
    }

    fn velocity_for_market(&self, market_id: &str) -> Decimal {
        self.recent_trades
            .iter()
            .filter(|(_, mid, _)| mid == market_id)
            .map(|(_, _, size)| size)
            .sum()
    }
}

impl BacktestStrategy for CopyTradeSlippage {
    fn name(&self) -> &str {
        "copy_trade"
    }

    fn on_book_snapshot(&mut self, _snap: &BookSnapshot) -> Vec<BacktestFill> {
        vec![]
    }

    fn on_trade(&mut self, trade: &TradeEvent) -> Vec<BacktestFill> {
        if !self.is_whale(&trade.maker) {
            return vec![];
        }

        // Track whale trade for velocity
        self.recent_trades.push_back((
            trade.timestamp,
            trade.market_id.clone(),
            trade.size,
        ));
        self.prune_old_trades(trade.timestamp);

        // Check velocity
        let velocity = self.velocity_for_market(&trade.market_id);
        if velocity < self.velocity_threshold {
            return vec![];
        }

        // Dedup check
        if let Some(last_ts) = self.dedup_map.get(&trade.market_id) {
            if (trade.timestamp - *last_ts).num_seconds() < self.dedup_ttl_secs {
                return vec![];
            }
        }
        self.dedup_map.insert(trade.market_id.clone(), trade.timestamp);

        // Simulate our fill with slippage
        let our_price = self.apply_slippage(trade.price);
        let size = self.max_order_size.min(trade.size);
        let fee = our_price * size * self.taker_fee_pct;

        // PnL is negative initially (we paid more than whale), resolution will determine final PnL
        // For backtest, assume the whale's directional bet was correct 60% of the time
        let slippage_cost = (our_price - trade.price) * size;

        vec![BacktestFill {
            timestamp: trade.timestamp,
            market_id: trade.market_id.clone(),
            side: trade.side.clone(),
            price: our_price,
            size,
            fee,
            pnl: -slippage_cost - fee, // Net cost of copying
            strategy: "copy_trade".to_string(),
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_trade(maker: &str, price: Decimal, size: Decimal, secs: u32) -> TradeEvent {
        TradeEvent {
            timestamp: NaiveDate::from_ymd_opt(2025, 1, 15)
                .unwrap()
                .and_hms_opt(10, 0, secs).unwrap(),
            market_id: "BTC-5m-up".to_string(),
            side: "BUY".to_string(),
            price,
            size,
            maker: maker.to_string(),
        }
    }

    #[test]
    fn test_ignores_non_whale() {
        let mut strat = CopyTradeSlippage::new(
            vec!["0xwhale1".to_string()],
            10, dec!(100), dec!(50), dec!(0.0156), dec!(50), 3600,
        );
        let trade = make_trade("0xrandom", dec!(0.50), dec!(200), 0);
        assert!(strat.on_trade(&trade).is_empty());
    }

    #[test]
    fn test_copies_whale_above_velocity() {
        let mut strat = CopyTradeSlippage::new(
            vec!["0xwhale1".to_string()],
            10, dec!(100), dec!(50), dec!(0.0156), dec!(50), 3600,
        );
        // First trade: size 50, below threshold
        let t1 = make_trade("0xwhale1", dec!(0.50), dec!(50), 0);
        assert!(strat.on_trade(&t1).is_empty());

        // Second trade: cumulative velocity = 50 + 60 = 110 > 100
        let t2 = make_trade("0xwhale1", dec!(0.51), dec!(60), 5);
        let fills = strat.on_trade(&t2);
        assert_eq!(fills.len(), 1);
        assert!(fills[0].price > dec!(0.51), "Should include slippage");
    }
}

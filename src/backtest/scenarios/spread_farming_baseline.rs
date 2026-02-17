use crate::backtest::engine::{BacktestFill, BacktestStrategy, BookSnapshot, TradeEvent};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Spread farming backtest: buy YES + NO when their combined ask < $1.00,
/// minus taker fees on both sides.
pub struct SpreadFarmingBaseline {
    taker_fee_pct: Decimal,
    max_order_size: Decimal,
}

impl SpreadFarmingBaseline {
    pub fn new(taker_fee_pct: Decimal, max_order_size: Decimal) -> Self {
        Self {
            taker_fee_pct,
            max_order_size,
        }
    }
}

impl BacktestStrategy for SpreadFarmingBaseline {
    fn name(&self) -> &str {
        "spread_farming"
    }

    fn on_book_snapshot(&mut self, snap: &BookSnapshot) -> Vec<BacktestFill> {
        let combined_ask = snap.yes_ask + snap.no_ask;

        // Both sides have taker fees
        let fee_per_side_yes = snap.yes_ask * self.taker_fee_pct;
        let fee_per_side_no = snap.no_ask * self.taker_fee_pct;
        let total_fee = fee_per_side_yes + fee_per_side_no;

        let cost = combined_ask + total_fee;

        // Arb exists if cost < $1.00 (guaranteed payout)
        if cost >= dec!(1.0) {
            return vec![];
        }

        let profit_per_share = dec!(1.0) - cost;
        let size = self.max_order_size.min(snap.bid_size).min(snap.ask_size);

        if size <= dec!(0) {
            return vec![];
        }

        let total_pnl = profit_per_share * size;
        let total_fee_amount = total_fee * size;

        vec![BacktestFill {
            timestamp: snap.timestamp,
            market_id: snap.market_id.clone(),
            side: "SPREAD_ARB".to_string(),
            price: combined_ask,
            size,
            fee: total_fee_amount,
            pnl: total_pnl,
            strategy: "spread_farming".to_string(),
        }]
    }

    fn on_trade(&mut self, _trade: &TradeEvent) -> Vec<BacktestFill> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_spread_arb_detected() {
        let mut strat = SpreadFarmingBaseline::new(dec!(0.0156), dec!(100));

        // YES ask = 0.45, NO ask = 0.52 => combined = 0.97
        // Fee = 0.45*0.0156 + 0.52*0.0156 = 0.01513
        // Cost = 0.97 + 0.01513 = 0.98513 < 1.00 => arb
        let snap = BookSnapshot {
            timestamp: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap().and_hms_opt(10, 0, 0).unwrap(),
            market_id: "BTC-5m-up".to_string(),
            best_bid: dec!(0.44),
            best_ask: dec!(0.45),
            bid_size: dec!(500),
            ask_size: dec!(500),
            yes_ask: dec!(0.45),
            no_ask: dec!(0.52),
        };

        let fills = strat.on_book_snapshot(&snap);
        assert_eq!(fills.len(), 1);
        assert!(fills[0].pnl > dec!(0), "Should be profitable");
    }

    #[test]
    fn test_no_arb_when_spread_too_wide() {
        let mut strat = SpreadFarmingBaseline::new(dec!(0.0156), dec!(100));

        // YES ask = 0.55, NO ask = 0.50 => combined = 1.05 > 1.00
        let snap = BookSnapshot {
            timestamp: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap().and_hms_opt(10, 0, 0).unwrap(),
            market_id: "BTC-5m-up".to_string(),
            best_bid: dec!(0.54),
            best_ask: dec!(0.55),
            bid_size: dec!(500),
            ask_size: dec!(500),
            yes_ask: dec!(0.55),
            no_ask: dec!(0.50),
        };

        let fills = strat.on_book_snapshot(&snap);
        assert!(fills.is_empty());
    }
}

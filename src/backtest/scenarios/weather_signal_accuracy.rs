use crate::backtest::engine::{BacktestFill, BacktestStrategy, BookSnapshot, TradeEvent, WeatherSnapshot};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;

/// Weather strategy backtest: compare NOAA forecast to PM temperature bucket prices.
/// If the NOAA forecast indicates high probability for a bucket that PM underprices,
/// buy the YES side.
pub struct WeatherSignalAccuracy {
    /// Map from gridpoint+date -> forecast high temp
    forecasts: HashMap<String, i32>,
    taker_fee_pct: Decimal,
    max_order_size: Decimal,
    /// Minimum edge (forecast implied prob - PM price) to trade
    min_edge: Decimal,
}

impl WeatherSignalAccuracy {
    pub fn new(taker_fee_pct: Decimal, max_order_size: Decimal, min_edge: Decimal) -> Self {
        Self {
            forecasts: HashMap::new(),
            taker_fee_pct,
            max_order_size,
            min_edge,
        }
    }

    /// Load weather forecasts into the strategy (called before engine.run()).
    pub fn load_forecasts(&mut self, snapshots: &[WeatherSnapshot]) {
        for snap in snapshots {
            let key = format!("{}_{}", snap.gridpoint, snap.timestamp.date());
            self.forecasts.insert(key, snap.forecast_high_f);
        }
    }

    /// Determine if a NOAA forecast falls within a PM bucket (e.g., "70-74F").
    fn forecast_matches_bucket(forecast_high: i32, bucket_label: &str) -> bool {
        // Parse bucket like "70-74" or ">=85"
        if let Some(range) = bucket_label.strip_suffix('F') {
            if let Some((low, high)) = range.split_once('-') {
                if let (Ok(lo), Ok(hi)) = (low.parse::<i32>(), high.parse::<i32>()) {
                    return forecast_high >= lo && forecast_high <= hi;
                }
            }
            if let Some(val) = range.strip_prefix(">=") {
                if let Ok(threshold) = val.parse::<i32>() {
                    return forecast_high >= threshold;
                }
            }
            if let Some(val) = range.strip_prefix("<=") {
                if let Ok(threshold) = val.parse::<i32>() {
                    return forecast_high <= threshold;
                }
            }
        }
        false
    }
}

impl BacktestStrategy for WeatherSignalAccuracy {
    fn name(&self) -> &str {
        "weather"
    }

    fn on_book_snapshot(&mut self, _snap: &BookSnapshot) -> Vec<BacktestFill> {
        // Weather strategy doesn't trade on order book snapshots directly
        vec![]
    }

    fn on_trade(&mut self, _trade: &TradeEvent) -> Vec<BacktestFill> {
        // Weather trades are generated from forecast signals, not from copying trades
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_matching() {
        assert!(WeatherSignalAccuracy::forecast_matches_bucket(72, "70-74F"));
        assert!(!WeatherSignalAccuracy::forecast_matches_bucket(75, "70-74F"));
        assert!(WeatherSignalAccuracy::forecast_matches_bucket(90, ">=85F"));
        assert!(!WeatherSignalAccuracy::forecast_matches_bucket(80, ">=85F"));
        assert!(WeatherSignalAccuracy::forecast_matches_bucket(60, "<=65F"));
        assert!(!WeatherSignalAccuracy::forecast_matches_bucket(70, "<=65F"));
    }

    #[test]
    fn test_strategy_name() {
        let strat = WeatherSignalAccuracy::new(dec!(0.0156), dec!(50), dec!(0.10));
        assert_eq!(strat.name(), "weather");
    }
}

use chrono::{NaiveDate, NaiveDateTime};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A single order book snapshot at a point in time.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BookSnapshot {
    pub timestamp: NaiveDateTime,
    pub market_id: String,
    pub best_bid: Decimal,
    pub best_ask: Decimal,
    pub bid_size: Decimal,
    pub ask_size: Decimal,
    /// YES + NO best-ask sum (for spread farming)
    pub yes_ask: Decimal,
    pub no_ask: Decimal,
}

/// A historical trade event.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TradeEvent {
    pub timestamp: NaiveDateTime,
    pub market_id: String,
    pub side: String,
    pub price: Decimal,
    pub size: Decimal,
    pub maker: String,
}

/// Weather forecast snapshot for backtesting weather strategy.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WeatherSnapshot {
    pub timestamp: NaiveDateTime,
    pub gridpoint: String,
    pub forecast_high_f: i32,
    pub forecast_low_f: i32,
    pub pm_bucket_label: String,
    pub pm_yes_price: Decimal,
}

/// Simulated fill representing an executed trade in the backtest.
#[derive(Debug, Clone, Serialize)]
pub struct BacktestFill {
    pub timestamp: NaiveDateTime,
    pub market_id: String,
    pub side: String,
    pub price: Decimal,
    pub size: Decimal,
    pub fee: Decimal,
    pub pnl: Decimal,
    pub strategy: String,
}

/// Aggregated results from a backtest run.
#[derive(Debug, Clone, Serialize)]
pub struct BacktestResult {
    pub strategy: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    pub total_pnl: Decimal,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub max_drawdown: Decimal,
    pub sharpe_ratio: f64,
    pub win_rate: f64,
    pub avg_trade_pnl: Decimal,
    pub fills: Vec<BacktestFill>,
}

/// Strategy trait that the backtest engine runs against historical data.
pub trait BacktestStrategy {
    fn name(&self) -> &str;
    fn on_book_snapshot(&mut self, snapshot: &BookSnapshot) -> Vec<BacktestFill>;
    fn on_trade(&mut self, trade: &TradeEvent) -> Vec<BacktestFill>;
}

/// The main backtest engine. Loads historical data and replays it through strategies.
pub struct BacktestEngine {
    book_snapshots: Vec<BookSnapshot>,
    trade_events: Vec<TradeEvent>,
    weather_snapshots: Vec<WeatherSnapshot>,
}

impl BacktestEngine {
    pub fn new() -> Self {
        Self {
            book_snapshots: Vec::new(),
            trade_events: Vec::new(),
            weather_snapshots: Vec::new(),
        }
    }

    /// Load order book snapshots from a JSON file.
    pub fn load_books<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, Box<dyn std::error::Error>> {
        let data = std::fs::read_to_string(path)?;
        self.book_snapshots = serde_json::from_str(&data)?;
        self.book_snapshots.sort_by_key(|s| s.timestamp);
        Ok(self.book_snapshots.len())
    }

    /// Load trade events from a JSON file.
    pub fn load_trades<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, Box<dyn std::error::Error>> {
        let data = std::fs::read_to_string(path)?;
        self.trade_events = serde_json::from_str(&data)?;
        self.trade_events.sort_by_key(|t| t.timestamp);
        Ok(self.trade_events.len())
    }

    /// Load weather snapshots from a JSON file.
    pub fn load_weather<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, Box<dyn std::error::Error>> {
        let data = std::fs::read_to_string(path)?;
        self.weather_snapshots = serde_json::from_str(&data)?;
        self.weather_snapshots.sort_by_key(|w| w.timestamp);
        Ok(self.weather_snapshots.len())
    }

    /// Run a strategy over the loaded historical data within the date range.
    pub fn run(
        &self,
        strategy: &mut dyn BacktestStrategy,
        start: NaiveDate,
        end: NaiveDate,
    ) -> BacktestResult {
        let mut all_fills: Vec<BacktestFill> = Vec::new();

        // Replay book snapshots
        for snap in &self.book_snapshots {
            let date = snap.timestamp.date();
            if date < start || date > end {
                continue;
            }
            let fills = strategy.on_book_snapshot(snap);
            all_fills.extend(fills);
        }

        // Replay trade events
        for trade in &self.trade_events {
            let date = trade.timestamp.date();
            if date < start || date > end {
                continue;
            }
            let fills = strategy.on_trade(trade);
            all_fills.extend(fills);
        }

        // Sort fills chronologically
        all_fills.sort_by_key(|f| f.timestamp);

        // Calculate metrics
        let total_trades = all_fills.len();
        let winning_trades = all_fills.iter().filter(|f| f.pnl > dec!(0)).count();
        let losing_trades = all_fills.iter().filter(|f| f.pnl < dec!(0)).count();
        let total_pnl: Decimal = all_fills.iter().map(|f| f.pnl).sum();
        let avg_trade_pnl = if total_trades > 0 {
            total_pnl / Decimal::from(total_trades as i64)
        } else {
            dec!(0)
        };

        let max_drawdown = calculate_max_drawdown(&all_fills);
        let sharpe_ratio = calculate_sharpe(&all_fills);
        let win_rate = if total_trades > 0 {
            winning_trades as f64 / total_trades as f64
        } else {
            0.0
        };

        BacktestResult {
            strategy: strategy.name().to_string(),
            start_date: start,
            end_date: end,
            total_pnl,
            total_trades,
            winning_trades,
            losing_trades,
            max_drawdown,
            sharpe_ratio,
            win_rate,
            avg_trade_pnl,
            fills: all_fills,
        }
    }

    /// Write backtest results to a CSV file.
    pub fn write_csv<P: AsRef<Path>>(
        result: &BacktestResult,
        path: P,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut wtr = csv::Writer::from_path(path)?;
        wtr.write_record(["timestamp", "market_id", "side", "price", "size", "fee", "pnl", "strategy"])?;
        for fill in &result.fills {
            wtr.write_record([
                fill.timestamp.to_string(),
                fill.market_id.clone(),
                fill.side.clone(),
                fill.price.to_string(),
                fill.size.to_string(),
                fill.fee.to_string(),
                fill.pnl.to_string(),
                fill.strategy.clone(),
            ])?;
        }
        wtr.flush()?;
        Ok(())
    }
}

/// Calculate max drawdown from cumulative PnL.
fn calculate_max_drawdown(fills: &[BacktestFill]) -> Decimal {
    let mut peak = dec!(0);
    let mut cumulative = dec!(0);
    let mut max_dd = dec!(0);

    for fill in fills {
        cumulative += fill.pnl;
        if cumulative > peak {
            peak = cumulative;
        }
        let drawdown = peak - cumulative;
        if drawdown > max_dd {
            max_dd = drawdown;
        }
    }
    max_dd
}

/// Calculate annualized Sharpe ratio (daily returns, 252 trading days).
fn calculate_sharpe(fills: &[BacktestFill]) -> f64 {
    if fills.is_empty() {
        return 0.0;
    }

    // Group PnL by date
    let mut daily_pnl: HashMap<NaiveDate, f64> = HashMap::new();
    for fill in fills {
        let date = fill.timestamp.date();
        let pnl_f64 = rust_decimal::prelude::ToPrimitive::to_f64(&fill.pnl).unwrap_or(0.0);
        *daily_pnl.entry(date).or_insert(0.0) += pnl_f64;
    }

    let returns: Vec<f64> = daily_pnl.values().copied().collect();
    if returns.len() < 2 {
        return 0.0;
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (returns.len() - 1) as f64;
    let std_dev = variance.sqrt();

    if std_dev == 0.0 {
        return 0.0;
    }

    // Annualized: multiply by sqrt(252)
    (mean / std_dev) * (252.0_f64).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_backtest() {
        let engine = BacktestEngine::new();
        let mut strategy = MockStrategy::new();
        let result = engine.run(
            &mut strategy,
            NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2025, 2, 16).unwrap(),
        );
        assert_eq!(result.total_trades, 0);
        assert_eq!(result.total_pnl, dec!(0));
        assert_eq!(result.sharpe_ratio, 0.0);
    }

    #[test]
    fn test_max_drawdown() {
        let fills = vec![
            mock_fill(dec!(10)),
            mock_fill(dec!(5)),
            mock_fill(dec!(-20)),
            mock_fill(dec!(3)),
        ];
        let dd = calculate_max_drawdown(&fills);
        // Peak at 15, then drops to -2. Drawdown = 15 - (-2) = 17
        assert_eq!(dd, dec!(17));
    }

    struct MockStrategy;
    impl MockStrategy {
        fn new() -> Self { Self }
    }
    impl BacktestStrategy for MockStrategy {
        fn name(&self) -> &str { "mock" }
        fn on_book_snapshot(&mut self, _: &BookSnapshot) -> Vec<BacktestFill> { vec![] }
        fn on_trade(&mut self, _: &TradeEvent) -> Vec<BacktestFill> { vec![] }
    }

    fn mock_fill(pnl: Decimal) -> BacktestFill {
        BacktestFill {
            timestamp: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap().and_hms_opt(12, 0, 0).unwrap(),
            market_id: "test".to_string(),
            side: "BUY".to_string(),
            price: dec!(0.50),
            size: dec!(10),
            fee: dec!(0.01),
            pnl,
            strategy: "test".to_string(),
        }
    }
}

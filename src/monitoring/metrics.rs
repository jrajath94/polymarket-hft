// Prometheus metrics for the HFT engine.
//
// Counters: orders_placed_total, trades_filled_total, trading_errors_total
// Gauges: portfolio_pnl, max_drawdown, ws_latency_ms
// Histograms: order_latency, fill_ratio

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Register all metric descriptions and return the Prometheus handle for scraping.
pub fn init_metrics() -> PrometheusHandle {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install prometheus recorder");

    // Counters
    describe_counter!("orders_placed_total", "Total number of orders placed");
    describe_counter!("trades_filled_total", "Total number of trades filled");
    describe_counter!("trading_errors_total", "Total number of trading errors");

    // Gauges
    describe_gauge!("portfolio_pnl", "Current portfolio PnL in USDC");
    describe_gauge!("max_drawdown", "Maximum drawdown as a fraction (0-1)");
    describe_gauge!("ws_latency_ms", "WebSocket latency in milliseconds");

    // Histograms
    describe_histogram!("order_latency_seconds", "Order placement latency in seconds");
    describe_histogram!("fill_ratio", "Fill ratio of orders (0-1)");

    handle
}

/// Increment the orders placed counter (strategy-labeled).
pub fn inc_orders_placed(strategy: &str) {
    let metric_name = format!("orders_placed_total[{}]", strategy);
    counter!(&metric_name).increment(1);
}

/// Increment the trades filled counter (strategy-labeled).
pub fn inc_trades_filled(strategy: &str) {
    let metric_name = format!("trades_filled_total[{}]", strategy);
    counter!(&metric_name).increment(1);
}

/// Increment the trading errors counter.
pub fn inc_trading_errors(strategy: &str, error_type: &str) {
    let metric_name = format!("trading_errors_total[{}:{}]", strategy, error_type);
    counter!(&metric_name).increment(1);
}

/// Set the portfolio PnL gauge.
pub fn set_portfolio_pnl(pnl: f64) {
    gauge!("portfolio_pnl").set(pnl);
}

/// Set the max drawdown gauge.
pub fn set_max_drawdown(drawdown: f64) {
    gauge!("max_drawdown").set(drawdown);
}

/// Set the WebSocket latency gauge.
pub fn set_ws_latency(latency_ms: f64) {
    gauge!("ws_latency_ms").set(latency_ms);
}

/// Record an order latency observation.
pub fn record_order_latency(seconds: f64) {
    histogram!("order_latency_seconds").record(seconds);
}

/// Record a fill ratio observation.
pub fn record_fill_ratio(ratio: f64) {
    histogram!("fill_ratio").record(ratio);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_init_and_render() {
        // init_metrics installs a global recorder, so we can only call it once per process.
        // In test, we use PrometheusBuilder directly to avoid conflicts.
        let handle = PrometheusBuilder::new()
            .install_recorder()
            .expect("failed to install recorder");

        // Register descriptions
        describe_counter!("orders_placed_total", "Total orders");
        describe_gauge!("portfolio_pnl", "PnL");
        describe_histogram!("order_latency_seconds", "Latency");

        // Record some values
        counter!("orders_placed_total[spread]").increment(5);
        gauge!("portfolio_pnl").set(123.45);
        histogram!("order_latency_seconds").record(0.042);

        let output = handle.render();

        // Verify prometheus text format is parseable and contains our metrics
        assert!(
            output.contains("orders_placed_total"),
            "output should contain orders_placed_total"
        );
        assert!(
            output.contains("portfolio_pnl"),
            "output should contain portfolio_pnl"
        );
        assert!(
            output.contains("order_latency_seconds"),
            "output should contain order_latency_seconds"
        );
    }

    #[test]
    fn test_prometheus_output_parseable() {
        // The output from render() should be valid Prometheus text format.
        // Each non-comment, non-empty line should have format: metric_name{labels} value
        let handle = PrometheusBuilder::new()
            .install_recorder()
            .unwrap_or_else(|_| {
                // Recorder already installed from previous test; that's fine for validation
                PrometheusBuilder::new()
                    .install_recorder()
                    .expect("fallback failed")
            });

        counter!("trades_filled_total[copy]").increment(1);

        let output = handle.render();
        for line in output.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue; // Comments and blank lines are valid
            }
            // Non-comment lines should contain a space separating metric from value
            assert!(
                line.contains(' '),
                "prometheus line should have metric and value: {}",
                line
            );
        }
    }
}

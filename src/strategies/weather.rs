// Weather strategy — trade NOAA temperature forecasts vs Polymarket weather markets.
//
// Flow:
// 1. NOAA forecast -> temperature for a time period
// 2. Map temperature to Polymarket bucket (e.g., "32-36F", "37-41F")
// 3. Compare forecast probability vs market price
// 4. If market is mispriced (forecast says 80% but market says 15%), emit GTC order
//
// Entry: market price < 0.15 for a bucket the forecast favors
// Exit: price > 0.45

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

use crate::noaa::client::ForecastPeriod;

/// A temperature bucket on Polymarket (e.g., "32-36F").
#[derive(Debug, Clone, PartialEq)]
pub struct TempBucket {
    pub label: String,
    pub low: i32,
    pub high: i32,
    pub asset_id: String,
}

/// Signal emitted by the weather strategy.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherSignal {
    pub asset_id: String,
    pub bucket_label: String,
    pub forecast_temp: i32,
    pub estimated_prob: Decimal,
    pub market_price: Decimal,
    pub order_type: WeatherOrderType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WeatherOrderType {
    /// Buy underpriced bucket with GTC order at market price
    BuyGtc { limit_price: Decimal },
    /// Sell overpriced position
    Sell { limit_price: Decimal },
}

/// Configuration for the weather strategy.
#[derive(Debug, Clone)]
pub struct WeatherStrategyConfig {
    /// Buy if market price is below this (e.g., 0.15)
    pub entry_threshold: Decimal,
    /// Sell if market price is above this (e.g., 0.45)
    pub exit_threshold: Decimal,
    /// Min edge (estimated_prob - market_price) to act
    pub min_edge: Decimal,
}

impl Default for WeatherStrategyConfig {
    fn default() -> Self {
        Self {
            entry_threshold: dec!(0.15),
            exit_threshold: dec!(0.45),
            min_edge: dec!(0.10),
        }
    }
}

/// Weather strategy engine.
pub struct WeatherStrategy {
    config: WeatherStrategyConfig,
}

impl WeatherStrategy {
    pub fn new(config: WeatherStrategyConfig) -> Self {
        Self { config }
    }

    /// Given a NOAA forecast period and a set of market buckets with prices,
    /// determine which buckets are mispriced and emit signals.
    pub fn evaluate(
        &self,
        forecast: &ForecastPeriod,
        buckets: &[(TempBucket, Decimal)], // (bucket, current_market_price)
    ) -> Vec<WeatherSignal> {
        let temp = forecast.temperature;
        let mut signals = Vec::new();

        for (bucket, market_price) in buckets {
            let estimated_prob = self.estimate_probability(temp, bucket);

            // Buy signal: market is cheap, forecast says likely
            if *market_price < self.config.entry_threshold
                && estimated_prob - *market_price >= self.config.min_edge
            {
                signals.push(WeatherSignal {
                    asset_id: bucket.asset_id.clone(),
                    bucket_label: bucket.label.clone(),
                    forecast_temp: temp,
                    estimated_prob,
                    market_price: *market_price,
                    order_type: WeatherOrderType::BuyGtc {
                        limit_price: *market_price,
                    },
                });
            }

            // Sell signal: we hold this and price has risen above exit
            if *market_price > self.config.exit_threshold && estimated_prob < *market_price {
                signals.push(WeatherSignal {
                    asset_id: bucket.asset_id.clone(),
                    bucket_label: bucket.label.clone(),
                    forecast_temp: temp,
                    estimated_prob,
                    market_price: *market_price,
                    order_type: WeatherOrderType::Sell {
                        limit_price: *market_price,
                    },
                });
            }
        }

        signals
    }

    /// Estimate the probability that the actual temperature falls in a bucket.
    ///
    /// Simple model: NOAA forecast has ~3F standard deviation for next-day forecasts.
    /// If the forecast temp is within the bucket, high probability.
    /// Probability drops off as forecast temp moves away from bucket range.
    fn estimate_probability(&self, forecast_temp: i32, bucket: &TempBucket) -> Decimal {
        let mid = (bucket.low + bucket.high) / 2;
        let distance = (forecast_temp - mid).unsigned_abs();
        let bucket_width = (bucket.high - bucket.low).max(1) as u32;

        if forecast_temp >= bucket.low && forecast_temp <= bucket.high {
            // Temp is inside the bucket
            dec!(0.60)
        } else if distance <= bucket_width {
            // One bucket away
            dec!(0.20)
        } else if distance <= bucket_width * 2 {
            // Two buckets away
            dec!(0.08)
        } else {
            // Far away
            dec!(0.02)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_buckets() -> Vec<(TempBucket, Decimal)> {
        vec![
            (
                TempBucket {
                    label: "27-31F".to_string(),
                    low: 27,
                    high: 31,
                    asset_id: "0xbucket_27_31".to_string(),
                },
                dec!(0.10), // Market says 10%
            ),
            (
                TempBucket {
                    label: "32-36F".to_string(),
                    low: 32,
                    high: 36,
                    asset_id: "0xbucket_32_36".to_string(),
                },
                dec!(0.12), // Market says 12%
            ),
            (
                TempBucket {
                    label: "37-41F".to_string(),
                    low: 37,
                    high: 41,
                    asset_id: "0xbucket_37_41".to_string(),
                },
                dec!(0.50), // Market says 50%
            ),
            (
                TempBucket {
                    label: "42-46F".to_string(),
                    low: 42,
                    high: 46,
                    asset_id: "0xbucket_42_46".to_string(),
                },
                dec!(0.08), // Market says 8%
            ),
        ]
    }

    fn make_forecast(temp: i32) -> ForecastPeriod {
        ForecastPeriod {
            name: "Tonight".to_string(),
            temperature: temp,
            temperature_unit: "F".to_string(),
            short_forecast: "Partly Cloudy".to_string(),
            start_time: "2026-02-16T18:00:00-05:00".to_string(),
            end_time: "2026-02-17T06:00:00-05:00".to_string(),
        }
    }

    #[test]
    fn test_mock_noaa_signal_emits_gtc() {
        let strategy = WeatherStrategy::new(WeatherStrategyConfig::default());
        let forecast = make_forecast(29); // 29F -> falls in 27-31F bucket
        let buckets = make_buckets();

        let signals = strategy.evaluate(&forecast, &buckets);

        // Should emit a BUY for 27-31F bucket (market=0.10, estimated~0.60, edge=0.50)
        let buy_signals: Vec<_> = signals
            .iter()
            .filter(|s| matches!(s.order_type, WeatherOrderType::BuyGtc { .. }))
            .collect();
        assert!(
            !buy_signals.is_empty(),
            "should emit GTC buy for the correct bucket"
        );
        assert_eq!(buy_signals[0].bucket_label, "27-31F");
        assert_eq!(buy_signals[0].forecast_temp, 29);
    }

    #[test]
    fn test_no_signal_for_already_priced_bucket() {
        let strategy = WeatherStrategy::new(WeatherStrategyConfig::default());
        let forecast = make_forecast(39); // 39F -> falls in 37-41F bucket
        let buckets = make_buckets();

        let signals = strategy.evaluate(&forecast, &buckets);

        // 37-41F bucket is priced at 0.50 (above entry_threshold of 0.15)
        // So no BUY signal for it
        let buy_37_41: Vec<_> = signals
            .iter()
            .filter(|s| s.bucket_label == "37-41F" && matches!(s.order_type, WeatherOrderType::BuyGtc { .. }))
            .collect();
        assert!(buy_37_41.is_empty(), "should not buy already-expensive bucket");
    }

    #[test]
    fn test_sell_signal_for_overpriced_bucket() {
        let strategy = WeatherStrategy::new(WeatherStrategyConfig::default());
        let forecast = make_forecast(29); // 29F -> far from 37-41F bucket
        let buckets = make_buckets();

        // 37-41F bucket is at 0.50 but forecast says temp=29 (far away, ~2%)
        // Should emit SELL signal since estimated < market_price and market > exit_threshold
        let sell_signals: Vec<_> = signals_of_type(&strategy.evaluate(&forecast, &buckets), "sell");
        assert!(
            sell_signals.iter().any(|s| s.bucket_label == "37-41F"),
            "should emit sell for overpriced distant bucket"
        );
    }

    #[test]
    fn test_probability_estimation() {
        let strategy = WeatherStrategy::new(WeatherStrategyConfig::default());
        let bucket = TempBucket {
            label: "32-36F".to_string(),
            low: 32,
            high: 36,
            asset_id: "0x".to_string(),
        };

        // Temp inside bucket
        assert_eq!(strategy.estimate_probability(34, &bucket), dec!(0.60));
        // One bucket away
        assert_eq!(strategy.estimate_probability(30, &bucket), dec!(0.20));
        // Two buckets away
        assert_eq!(strategy.estimate_probability(25, &bucket), dec!(0.08));
        // Far away
        assert_eq!(strategy.estimate_probability(10, &bucket), dec!(0.02));
    }

    // Helper to filter signals by type
    fn signals_of_type<'a>(signals: &'a [WeatherSignal], kind: &str) -> Vec<&'a WeatherSignal> {
        signals
            .iter()
            .filter(|s| match kind {
                "buy" => matches!(s.order_type, WeatherOrderType::BuyGtc { .. }),
                "sell" => matches!(s.order_type, WeatherOrderType::Sell { .. }),
                _ => false,
            })
            .collect()
    }
}

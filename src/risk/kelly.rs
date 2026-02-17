// Kelly criterion position sizing for prediction markets.
//
// Formula: f* = (p_true - price) / (1 - price) * kelly_fraction
// where kelly_fraction = 0.25 (quarter-Kelly for safety).
//
// Never risk more than max_fraction of bankroll on a single trade.

use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Kelly criterion position sizer.
pub struct KellySizer {
    /// Fraction of full Kelly to use (e.g., 0.25 for quarter-Kelly)
    kelly_fraction: Decimal,
    /// Absolute max fraction of bankroll for any single trade
    max_fraction: Decimal,
}

impl KellySizer {
    pub fn new(kelly_fraction: Decimal, max_fraction: Decimal) -> Self {
        Self {
            kelly_fraction,
            max_fraction,
        }
    }

    /// Calculate the optimal position size as a fraction of bankroll.
    ///
    /// - `p_true`: estimated true probability of the outcome (0-1)
    /// - `price`: current market price (0-1)
    /// - `bankroll`: total available capital in USDC
    ///
    /// Returns the dollar amount to wager, or 0 if no edge exists.
    pub fn size(&self, p_true: Decimal, price: Decimal, bankroll: Decimal) -> Decimal {
        // No edge if our estimate <= market price
        if p_true <= price {
            return dec!(0);
        }

        // Avoid division by zero when price = 1
        if price >= dec!(1) {
            return dec!(0);
        }

        // f* = (p_true - price) / (1 - price) * kelly_fraction
        let raw_fraction = (p_true - price) / (dec!(1) - price) * self.kelly_fraction;

        // Clamp to max_fraction
        let clamped = raw_fraction.min(self.max_fraction);

        // Never negative
        let fraction = clamped.max(dec!(0));

        fraction * bankroll
    }

    /// Calculate fraction only (without multiplying by bankroll).
    pub fn fraction(&self, p_true: Decimal, price: Decimal) -> Decimal {
        if p_true <= price || price >= dec!(1) {
            return dec!(0);
        }

        let raw = (p_true - price) / (dec!(1) - price) * self.kelly_fraction;
        raw.min(self.max_fraction).max(dec!(0))
    }
}

impl Default for KellySizer {
    fn default() -> Self {
        Self {
            kelly_fraction: dec!(0.25),
            max_fraction: dec!(0.25),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_sizer() -> KellySizer {
        KellySizer::default()
    }

    // Table-driven tests for edge cases
    #[test]
    fn test_kelly_table_driven() {
        let sizer = default_sizer();
        let bankroll = dec!(1000);

        struct TestCase {
            name: &'static str,
            p_true: Decimal,
            price: Decimal,
            expected_min: Decimal,
            expected_max: Decimal,
        }

        let cases = vec![
            TestCase {
                name: "strong edge: p=0.80, price=0.50",
                p_true: dec!(0.80),
                price: dec!(0.50),
                expected_min: dec!(140),  // (0.8-0.5)/(1-0.5)*0.25*1000 = 150
                expected_max: dec!(160),
            },
            TestCase {
                name: "no edge: p=0.50, price=0.60",
                p_true: dec!(0.50),
                price: dec!(0.60),
                expected_min: dec!(0),
                expected_max: dec!(0),
            },
            TestCase {
                name: "equal: p=0.50, price=0.50",
                p_true: dec!(0.50),
                price: dec!(0.50),
                expected_min: dec!(0),
                expected_max: dec!(0),
            },
            TestCase {
                name: "tiny edge: p=0.52, price=0.50",
                p_true: dec!(0.52),
                price: dec!(0.50),
                // (0.02)/(0.50)*0.25*1000 = 10
                expected_min: dec!(9),
                expected_max: dec!(11),
            },
            TestCase {
                name: "high price: p=0.95, price=0.90",
                p_true: dec!(0.95),
                price: dec!(0.90),
                // (0.05)/(0.10)*0.25*1000 = 125
                expected_min: dec!(120),
                expected_max: dec!(130),
            },
            TestCase {
                name: "extreme edge capped at max: p=0.99, price=0.10",
                p_true: dec!(0.99),
                price: dec!(0.10),
                // raw = (0.89)/(0.90)*0.25 = ~0.247, capped at 0.25
                expected_min: dec!(240),
                expected_max: dec!(250),
            },
        ];

        for tc in cases {
            let result = sizer.size(tc.p_true, tc.price, bankroll);
            assert!(
                result >= tc.expected_min && result <= tc.expected_max,
                "{}: got {}, expected [{}, {}]",
                tc.name,
                result,
                tc.expected_min,
                tc.expected_max
            );
        }
    }

    #[test]
    fn test_never_exceeds_max_fraction() {
        let sizer = default_sizer();
        let bankroll = dec!(10000);

        // Even with near-certain probability, should not exceed 25% of bankroll
        let result = sizer.size(dec!(0.999), dec!(0.01), bankroll);
        let max_allowed = dec!(0.25) * bankroll;
        assert!(
            result <= max_allowed,
            "kelly should cap at 25% of bankroll: got {}, max {}",
            result,
            max_allowed
        );
    }

    #[test]
    fn test_zero_bankroll() {
        let sizer = default_sizer();
        let result = sizer.size(dec!(0.80), dec!(0.50), dec!(0));
        assert_eq!(result, dec!(0));
    }

    #[test]
    fn test_price_at_one() {
        let sizer = default_sizer();
        let result = sizer.size(dec!(0.99), dec!(1.0), dec!(1000));
        assert_eq!(result, dec!(0), "price=1 should return 0");
    }

    #[test]
    fn test_fraction_calculation() {
        let sizer = default_sizer();
        let f = sizer.fraction(dec!(0.80), dec!(0.50));
        // (0.30/0.50)*0.25 = 0.15
        assert_eq!(f, dec!(0.15));
    }

    #[test]
    fn test_custom_kelly_fraction() {
        // Half-Kelly instead of quarter
        let sizer = KellySizer::new(dec!(0.50), dec!(0.25));
        let f = sizer.fraction(dec!(0.80), dec!(0.50));
        // (0.30/0.50)*0.50 = 0.30, but capped at 0.25
        assert_eq!(f, dec!(0.25));
    }
}

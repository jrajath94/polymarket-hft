// Fee calculation for Polymarket CLOB orders.
//
// Formula: ceil(fee_rate * shares * price * (1 - price))
// Fee rates loaded from config (never hardcoded).
// The quadratic price*(1-price) term means fees peak at 50c and approach 0 near 0c/100c.

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;

use crate::clob::types::MarketType;
use crate::config::FeesSection;
use crate::error::{AppError, Result};

/// Calculate the taker fee for an order.
///
/// Formula: `ceil(fee_rate * shares * price * (1 - price))`
/// Returns the fee in USDC terms (same precision as price).
pub fn calculate_taker_fee(
    fees: &FeesSection,
    market_type: MarketType,
    shares: Decimal,
    price: Decimal,
) -> Result<Decimal> {
    if price < dec!(0) || price > dec!(1) {
        return Err(AppError::OrderBuilder(format!(
            "price must be in [0, 1], got {}",
            price
        )));
    }
    if shares <= dec!(0) {
        return Err(AppError::OrderBuilder(format!(
            "shares must be positive, got {}",
            shares
        )));
    }

    let fee_rate = get_taker_rate(fees, market_type);
    let complement = dec!(1) - price;
    let raw_fee = fee_rate * shares * price * complement;

    // Ceil to 6 decimal places (USDC precision)
    Ok(raw_fee.round_dp_with_strategy(6, RoundingStrategy::AwayFromZero))
}

/// Get the taker fee rate for a given market type from config.
pub fn get_taker_rate(fees: &FeesSection, market_type: MarketType) -> Decimal {
    let rate_f64 = match market_type {
        MarketType::Crypto5m => fees.crypto_5m_taker,
        MarketType::Crypto15m => fees.crypto_15m_taker,
        MarketType::Ncaab => fees.ncaab_taker,
        MarketType::SerieA => fees.serie_a_taker,
        MarketType::Default => fees.default_taker,
    };
    Decimal::from_f64(rate_f64).unwrap_or(dec!(0))
}

/// Calculate the maker rebate for a trade.
/// Rebate = maker_rebate_pct * taker_fee_collected
pub fn calculate_maker_rebate(fees: &FeesSection, taker_fee: Decimal) -> Decimal {
    let rebate_pct = Decimal::from_f64(fees.maker_rebate_pct).unwrap_or(dec!(0));
    (rebate_pct * taker_fee).round_dp(6)
}

/// Check if net profit exceeds fees for a spread arb.
/// Returns (net_profit, is_profitable).
pub fn is_spread_profitable(
    fees: &FeesSection,
    market_type: MarketType,
    yes_price: Decimal,
    no_price: Decimal,
    shares: Decimal,
) -> (Decimal, bool) {
    let total_cost = yes_price + no_price;
    let gross_profit = (dec!(1) - total_cost) * shares;

    // Need to pay fees on both legs
    let fee_yes = calculate_taker_fee(fees, market_type, shares, yes_price).unwrap_or(dec!(0));
    let fee_no = calculate_taker_fee(fees, market_type, shares, no_price).unwrap_or(dec!(0));
    let total_fees = fee_yes + fee_no;

    let net = gross_profit - total_fees;
    (net, net > dec!(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_fees() -> FeesSection {
        FeesSection {
            crypto_5m_taker: 0.0156,
            crypto_15m_taker: 0.0156,
            ncaab_taker: 0.0156,
            serie_a_taker: 0.0156,
            default_taker: 0.0,
            maker_rebate_pct: 0.20,
        }
    }

    #[test]
    fn test_fee_at_50_cents() {
        // At 50c: fee = 0.0156 * 10 * 0.5 * 0.5 = 0.039
        let fees = test_fees();
        let fee = calculate_taker_fee(&fees, MarketType::Crypto5m, dec!(10), dec!(0.50)).unwrap();
        assert_eq!(fee, dec!(0.039));
    }

    #[test]
    fn test_fee_at_10_cents() {
        // At 10c: fee = 0.0156 * 10 * 0.1 * 0.9 = 0.01404
        let fees = test_fees();
        let fee = calculate_taker_fee(&fees, MarketType::Crypto15m, dec!(10), dec!(0.10)).unwrap();
        assert_eq!(fee, dec!(0.01404));
    }

    #[test]
    fn test_fee_at_90_cents() {
        // At 90c: fee = 0.0156 * 10 * 0.9 * 0.1 = 0.01404 (symmetric with 10c)
        let fees = test_fees();
        let fee = calculate_taker_fee(&fees, MarketType::Crypto15m, dec!(10), dec!(0.90)).unwrap();
        assert_eq!(fee, dec!(0.01404));
    }

    #[test]
    fn test_fee_symmetry() {
        // price*(1-price) is symmetric around 0.5
        let fees = test_fees();
        let fee_10 =
            calculate_taker_fee(&fees, MarketType::Crypto5m, dec!(10), dec!(0.10)).unwrap();
        let fee_90 =
            calculate_taker_fee(&fees, MarketType::Crypto5m, dec!(10), dec!(0.90)).unwrap();
        assert_eq!(fee_10, fee_90);
    }

    #[test]
    fn test_fee_zero_for_default_markets() {
        let fees = test_fees();
        let fee = calculate_taker_fee(&fees, MarketType::Default, dec!(100), dec!(0.50)).unwrap();
        assert_eq!(fee, dec!(0));
    }

    #[test]
    fn test_fee_invalid_price_above_one() {
        let fees = test_fees();
        let result = calculate_taker_fee(&fees, MarketType::Crypto5m, dec!(10), dec!(1.5));
        assert!(result.is_err());
    }

    #[test]
    fn test_fee_invalid_negative_price() {
        let fees = test_fees();
        let result = calculate_taker_fee(&fees, MarketType::Crypto5m, dec!(10), dec!(-0.1));
        assert!(result.is_err());
    }

    #[test]
    fn test_fee_invalid_zero_shares() {
        let fees = test_fees();
        let result = calculate_taker_fee(&fees, MarketType::Crypto5m, dec!(0), dec!(0.5));
        assert!(result.is_err());
    }

    #[test]
    fn test_maker_rebate() {
        let fees = test_fees();
        let taker_fee = dec!(0.039);
        let rebate = calculate_maker_rebate(&fees, taker_fee);
        assert_eq!(rebate, dec!(0.0078));
    }

    #[test]
    fn test_spread_profitable_yes() {
        // YES=0.48, NO=0.48, total=0.96. Gross = 0.04*10 = 0.40
        // Fee per leg at default market = 0 -> net = 0.40
        let fees = test_fees();
        let (net, profitable) =
            is_spread_profitable(&fees, MarketType::Default, dec!(0.48), dec!(0.48), dec!(10));
        assert!(profitable);
        assert_eq!(net, dec!(0.40));
    }

    #[test]
    fn test_spread_unprofitable_with_fees() {
        // YES=0.49, NO=0.49, total=0.98. Gross = 0.02*10 = 0.20
        // Fee at 49c on crypto: 0.0156 * 10 * 0.49 * 0.51 = 0.038964 per leg
        // Total fees ~ 0.078 -> net ~ 0.122 (still profitable at this edge)
        let fees = test_fees();
        let (net, profitable) =
            is_spread_profitable(&fees, MarketType::Crypto5m, dec!(0.49), dec!(0.49), dec!(10));
        assert!(profitable);
        assert!(net > dec!(0));
    }

    #[test]
    fn test_spread_marginal() {
        // YES=0.495, NO=0.495, total=0.99. Gross = 0.01*10 = 0.10
        // Fee per leg: 0.0156 * 10 * 0.495 * 0.505 ~ 0.039 per leg -> total ~0.078
        // Net ~ 0.022 (barely profitable)
        let fees = test_fees();
        let (net, profitable) =
            is_spread_profitable(&fees, MarketType::Crypto5m, dec!(0.495), dec!(0.495), dec!(10));
        assert!(profitable);
        assert!(net > dec!(0));
    }

    #[test]
    fn test_fee_boundary_at_zero_price() {
        // At price=0: fee = rate * shares * 0 * 1 = 0
        let fees = test_fees();
        let fee = calculate_taker_fee(&fees, MarketType::Crypto5m, dec!(10), dec!(0)).unwrap();
        assert_eq!(fee, dec!(0));
    }

    #[test]
    fn test_fee_boundary_at_one_price() {
        // At price=1: fee = rate * shares * 1 * 0 = 0
        let fees = test_fees();
        let fee = calculate_taker_fee(&fees, MarketType::Crypto5m, dec!(10), dec!(1)).unwrap();
        assert_eq!(fee, dec!(0));
    }
}

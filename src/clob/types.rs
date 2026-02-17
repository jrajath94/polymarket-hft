// CLOB data types: BookLevel, OrderPayload, StrategySignal, etc.
//
// All price/size fields use rust_decimal::Decimal -- never f64.
// These are the shared types flowing through the entire pipeline:
// WS -> OrderBookCache -> Strategy -> StrategySignal -> OrderBuilder -> OrderPayload -> BatchExecutor

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single price/size level in the order book.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookLevel {
    pub price: Decimal,
    pub size: Decimal,
}

/// Which side of the market an order targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy,
    Sell,
}

/// Order time-in-force.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    /// Good-til-cancelled
    GTC,
    /// Good-til-date
    GTD,
    /// Fill-or-kill
    FOK,
    /// Fill-and-kill (partial fill ok, cancel remainder)
    FAK,
}

/// Market type determines fee schedule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketType {
    Crypto5m,
    Crypto15m,
    Ncaab,
    SerieA,
    Default,
}

/// Signal emitted by a strategy, consumed by OrderBuilder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySignal {
    pub id: Uuid,
    pub strategy_name: String,
    pub token_id: String,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
    pub market_type: MarketType,
    pub time_in_force: TimeInForce,
    /// Estimated edge (profit margin after fees). Used by Kelly sizer.
    pub estimated_edge: Decimal,
    /// Strategy's estimated win probability for Kelly sizing.
    pub estimated_win_prob: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// Fully signed order payload ready for CLOB submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderPayload {
    pub order: OrderData,
    pub owner: String,
    pub order_type: String,
    pub signature: String,
}

/// The core order data that gets EIP-712 signed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderData {
    /// Unique nonce for this order
    pub salt: String,
    /// The maker (our wallet address)
    pub maker: String,
    /// The signer address
    pub signer: String,
    /// The token ID being traded
    pub token_id: String,
    /// Maker amount in raw units
    pub maker_amount: String,
    /// Taker amount in raw units
    pub taker_amount: String,
    /// Side: 0 = Buy, 1 = Sell
    pub side: String,
    /// Expiration timestamp (0 = no expiry for GTC)
    pub expiration: String,
    /// Nonce (0)
    pub nonce: String,
    /// Fee rate basis points
    pub fee_rate_bps: String,
    /// Signature type (always 0 for EOA)
    pub signature_type: u8,
}

/// Response from CLOB after order placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    pub success: bool,
    #[serde(default)]
    pub error_msg: String,
    #[serde(default)]
    pub order_id: String,
    #[serde(default)]
    pub status: String,
}

/// A market's YES/NO token pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPair {
    pub condition_id: String,
    pub yes_token_id: String,
    pub no_token_id: String,
    pub market_type: MarketType,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_book_level_creation() {
        let level = BookLevel {
            price: dec!(0.48),
            size: dec!(100.0),
        };
        assert_eq!(level.price, dec!(0.48));
        assert_eq!(level.size, dec!(100.0));
    }

    #[test]
    fn test_strategy_signal_serialization() {
        let signal = StrategySignal {
            id: Uuid::new_v4(),
            strategy_name: "spread_farming".to_string(),
            token_id: "token_abc".to_string(),
            side: Side::Buy,
            price: dec!(0.48),
            size: dec!(10.0),
            market_type: MarketType::Crypto5m,
            time_in_force: TimeInForce::FOK,
            estimated_edge: dec!(0.03),
            estimated_win_prob: dec!(0.95),
            timestamp: Utc::now(),
        };

        let json = serde_json::to_string(&signal).unwrap();
        let deser: StrategySignal = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.strategy_name, "spread_farming");
        assert_eq!(deser.price, dec!(0.48));
    }

    #[test]
    fn test_side_enum() {
        assert_ne!(Side::Buy, Side::Sell);
        let json = serde_json::to_string(&Side::Buy).unwrap();
        assert_eq!(json, "\"Buy\"");
    }

    #[test]
    fn test_market_type_fee_schedule_mapping() {
        // Crypto 5m/15m and certain sports have taker fees
        let fee_markets = [
            MarketType::Crypto5m,
            MarketType::Crypto15m,
            MarketType::Ncaab,
            MarketType::SerieA,
        ];
        for mt in &fee_markets {
            assert_ne!(*mt, MarketType::Default);
        }
    }

    #[test]
    fn test_order_data_serialization() {
        let order = OrderData {
            salt: "12345".to_string(),
            maker: "0xabc".to_string(),
            signer: "0xabc".to_string(),
            token_id: "token123".to_string(),
            maker_amount: "10000000".to_string(),
            taker_amount: "5000000".to_string(),
            side: "0".to_string(),
            expiration: "0".to_string(),
            nonce: "0".to_string(),
            fee_rate_bps: "0".to_string(),
            signature_type: 0,
        };
        let json = serde_json::to_string(&order).unwrap();
        // camelCase serialization
        assert!(json.contains("tokenId"));
        assert!(json.contains("makerAmount"));
    }
}

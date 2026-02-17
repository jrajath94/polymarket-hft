// Data API client for Polymarket — used by copy trading to poll whale trades.
//
// Endpoint: GET https://data-api.polymarket.com/trades?user={wallet}
// Rate limit: 200 req/10s on /trades

use chrono::{DateTime, Utc};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

/// A single trade event from the Data API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TradeEvent {
    pub id: String,
    pub asset_id: String,
    pub market: String,
    pub side: TradeSide,
    pub size: Decimal,
    pub price: Decimal,
    pub timestamp: DateTime<Utc>,
    pub user: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum TradeSide {
    Buy,
    Sell,
}

/// Client for Polymarket's Data API (read-only trade data).
#[derive(Clone)]
pub struct DataApiClient {
    http: Client,
    base_url: String,
}

impl DataApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Fetch recent trades for a given wallet address.
    pub async fn get_trades(&self, wallet: &str) -> Result<Vec<TradeEvent>> {
        let url = format!("{}/trades", self.base_url);
        let resp = self
            .http
            .get(&url)
            .query(&[("user", wallet)])
            .send()
            .await
            .map_err(|e| AppError::Http(format!("data api request failed: {}", e)))?;

        if !resp.status().is_success() {
            return Err(AppError::Http(format!(
                "data api returned status {}",
                resp.status()
            )));
        }

        let trades: Vec<TradeEvent> = resp
            .json()
            .await
            .map_err(|e| AppError::Http(format!("failed to parse trades: {}", e)))?;

        Ok(trades)
    }

    /// Fetch trades filtered by minimum size.
    pub async fn get_trades_filtered(
        &self,
        wallet: &str,
        min_size: Decimal,
    ) -> Result<Vec<TradeEvent>> {
        let trades = self.get_trades(wallet).await?;
        Ok(trades
            .into_iter()
            .filter(|t| t.size >= min_size)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn sample_trade_json() -> &'static str {
        r#"[
            {
                "id": "trade_001",
                "asset_id": "0xabc123",
                "market": "Will BTC be above 100k?",
                "side": "BUY",
                "size": "50.00",
                "price": "0.65",
                "timestamp": "2026-02-16T12:00:00Z",
                "user": "0xWhale1"
            },
            {
                "id": "trade_002",
                "asset_id": "0xdef456",
                "market": "Will ETH be above 5k?",
                "side": "SELL",
                "size": "3.00",
                "price": "0.40",
                "timestamp": "2026-02-16T12:01:00Z",
                "user": "0xWhale1"
            }
        ]"#
    }

    #[test]
    fn test_parse_trade_events() {
        let trades: Vec<TradeEvent> = serde_json::from_str(sample_trade_json()).unwrap();
        assert_eq!(trades.len(), 2);

        assert_eq!(trades[0].id, "trade_001");
        assert_eq!(trades[0].side, TradeSide::Buy);
        assert_eq!(trades[0].size, dec!(50.00));
        assert_eq!(trades[0].price, dec!(0.65));
        assert_eq!(trades[0].user, "0xWhale1");

        assert_eq!(trades[1].side, TradeSide::Sell);
        assert_eq!(trades[1].size, dec!(3.00));
    }

    #[test]
    fn test_trade_side_deserialization() {
        let buy: TradeSide = serde_json::from_str(r#""BUY""#).unwrap();
        assert_eq!(buy, TradeSide::Buy);

        let sell: TradeSide = serde_json::from_str(r#""SELL""#).unwrap();
        assert_eq!(sell, TradeSide::Sell);
    }

    #[tokio::test]
    async fn test_get_trades_mock_server() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/trades")
            .match_query(mockito::Matcher::UrlEncoded("user".into(), "0xWhale1".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(sample_trade_json())
            .create_async()
            .await;

        let client = DataApiClient::new(&server.url());
        let trades = client.get_trades("0xWhale1").await.unwrap();

        assert_eq!(trades.len(), 2);
        assert_eq!(trades[0].id, "trade_001");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_get_trades_filtered_by_size() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/trades")
            .match_query(mockito::Matcher::UrlEncoded("user".into(), "0xWhale1".into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(sample_trade_json())
            .create_async()
            .await;

        let client = DataApiClient::new(&server.url());
        let trades = client
            .get_trades_filtered("0xWhale1", dec!(10.0))
            .await
            .unwrap();

        // Only the $50 trade passes the $10 min size filter
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].id, "trade_001");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_get_trades_server_error() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/trades")
            .match_query(mockito::Matcher::UrlEncoded("user".into(), "0xWhale1".into()))
            .with_status(500)
            .with_body("internal server error")
            .create_async()
            .await;

        let client = DataApiClient::new(&server.url());
        let result = client.get_trades("0xWhale1").await;

        assert!(result.is_err());
        mock.assert_async().await;
    }
}

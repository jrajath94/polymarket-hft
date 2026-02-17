// In-memory order book cache using DashMap for lock-free reads.
//
// The cache stores bids/asks per asset_id. The WS client writes updates,
// strategies read best bid/ask. DashMap provides sharded locking so
// readers and writers rarely contend.

use dashmap::DashMap;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use std::time::Instant;

use crate::clob::types::BookLevel;

/// Snapshot of an order book for one asset.
#[derive(Debug, Clone)]
pub struct BookSnapshot {
    pub bids: Vec<BookLevel>,
    pub asks: Vec<BookLevel>,
    pub last_update: Instant,
}

/// Concurrent in-memory order book cache.
#[derive(Debug, Clone)]
pub struct OrderBookCache {
    books: Arc<DashMap<String, BookSnapshot>>,
}

impl OrderBookCache {
    pub fn new() -> Self {
        Self {
            books: Arc::new(DashMap::new()),
        }
    }

    /// Update the full book for an asset. Bids should be sorted descending, asks ascending.
    pub fn update_book(&self, asset_id: &str, bids: Vec<BookLevel>, asks: Vec<BookLevel>) {
        self.books.insert(
            asset_id.to_string(),
            BookSnapshot {
                bids,
                asks,
                last_update: Instant::now(),
            },
        );
    }

    /// Get the best bid price for an asset.
    pub fn best_bid(&self, asset_id: &str) -> Option<Decimal> {
        self.books
            .get(asset_id)
            .and_then(|snap| snap.bids.first().map(|l| l.price))
    }

    /// Get the best ask price for an asset.
    pub fn best_ask(&self, asset_id: &str) -> Option<Decimal> {
        self.books
            .get(asset_id)
            .and_then(|snap| snap.asks.first().map(|l| l.price))
    }

    /// Get the best bid size for an asset.
    pub fn best_bid_size(&self, asset_id: &str) -> Option<Decimal> {
        self.books
            .get(asset_id)
            .and_then(|snap| snap.bids.first().map(|l| l.size))
    }

    /// Get the best ask size for an asset.
    pub fn best_ask_size(&self, asset_id: &str) -> Option<Decimal> {
        self.books
            .get(asset_id)
            .and_then(|snap| snap.asks.first().map(|l| l.size))
    }

    /// Get the mid price for an asset.
    pub fn mid_price(&self, asset_id: &str) -> Option<Decimal> {
        let bid = self.best_bid(asset_id)?;
        let ask = self.best_ask(asset_id)?;
        Some((bid + ask) / dec!(2))
    }

    /// Get the spread for an asset (ask - bid).
    pub fn spread(&self, asset_id: &str) -> Option<Decimal> {
        let bid = self.best_bid(asset_id)?;
        let ask = self.best_ask(asset_id)?;
        Some(ask - bid)
    }

    /// Get the full book snapshot.
    pub fn get_snapshot(&self, asset_id: &str) -> Option<BookSnapshot> {
        self.books.get(asset_id).map(|s| s.clone())
    }

    /// Check if a book is stale (> threshold_ms since last update).
    pub fn is_stale(&self, asset_id: &str, threshold_ms: u64) -> bool {
        match self.books.get(asset_id) {
            Some(snap) => snap.last_update.elapsed().as_millis() > threshold_ms as u128,
            None => true,
        }
    }

    /// Remove a book from cache.
    pub fn remove(&self, asset_id: &str) {
        self.books.remove(asset_id);
    }

    /// Number of books currently cached.
    pub fn len(&self) -> usize {
        self.books.len()
    }

    /// Check if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.books.is_empty()
    }
}

impl Default for OrderBookCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bids(prices: &[&str]) -> Vec<BookLevel> {
        prices
            .iter()
            .map(|p| BookLevel {
                price: p.parse().unwrap(),
                size: dec!(100),
            })
            .collect()
    }

    fn make_asks(prices: &[&str]) -> Vec<BookLevel> {
        prices
            .iter()
            .map(|p| BookLevel {
                price: p.parse().unwrap(),
                size: dec!(50),
            })
            .collect()
    }

    #[test]
    fn test_update_and_read_book() {
        let cache = OrderBookCache::new();
        cache.update_book("yes_token", make_bids(&["0.48", "0.47"]), make_asks(&["0.52", "0.53"]));

        assert_eq!(cache.best_bid("yes_token"), Some(dec!(0.48)));
        assert_eq!(cache.best_ask("yes_token"), Some(dec!(0.52)));
    }

    #[test]
    fn test_missing_asset_returns_none() {
        let cache = OrderBookCache::new();
        assert_eq!(cache.best_bid("nonexistent"), None);
        assert_eq!(cache.best_ask("nonexistent"), None);
        assert_eq!(cache.mid_price("nonexistent"), None);
    }

    #[test]
    fn test_mid_price() {
        let cache = OrderBookCache::new();
        cache.update_book("tok", make_bids(&["0.48"]), make_asks(&["0.52"]));
        assert_eq!(cache.mid_price("tok"), Some(dec!(0.50)));
    }

    #[test]
    fn test_spread() {
        let cache = OrderBookCache::new();
        cache.update_book("tok", make_bids(&["0.48"]), make_asks(&["0.52"]));
        assert_eq!(cache.spread("tok"), Some(dec!(0.04)));
    }

    #[test]
    fn test_book_update_replaces_previous() {
        let cache = OrderBookCache::new();
        cache.update_book("tok", make_bids(&["0.48"]), make_asks(&["0.52"]));
        cache.update_book("tok", make_bids(&["0.50"]), make_asks(&["0.51"]));
        assert_eq!(cache.best_bid("tok"), Some(dec!(0.50)));
    }

    #[test]
    fn test_staleness_check() {
        let cache = OrderBookCache::new();
        // Nonexistent = stale
        assert!(cache.is_stale("tok", 1000));
        // Just updated = not stale
        cache.update_book("tok", make_bids(&["0.48"]), make_asks(&["0.52"]));
        assert!(!cache.is_stale("tok", 1000));
    }

    #[test]
    fn test_cache_len_and_remove() {
        let cache = OrderBookCache::new();
        assert!(cache.is_empty());
        cache.update_book("a", vec![], vec![]);
        cache.update_book("b", vec![], vec![]);
        assert_eq!(cache.len(), 2);
        cache.remove("a");
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_best_bid_size() {
        let cache = OrderBookCache::new();
        cache.update_book("tok", make_bids(&["0.48"]), make_asks(&["0.52"]));
        assert_eq!(cache.best_bid_size("tok"), Some(dec!(100)));
        assert_eq!(cache.best_ask_size("tok"), Some(dec!(50)));
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;
        let cache = OrderBookCache::new();
        let cache_clone = cache.clone();

        let writer = thread::spawn(move || {
            for i in 0..100 {
                cache_clone.update_book(
                    "tok",
                    vec![BookLevel {
                        price: Decimal::from(i),
                        size: dec!(10),
                    }],
                    vec![BookLevel {
                        price: Decimal::from(i + 1),
                        size: dec!(10),
                    }],
                );
            }
        });

        // Read while writing
        for _ in 0..100 {
            let _ = cache.best_bid("tok");
            let _ = cache.best_ask("tok");
        }

        writer.join().unwrap();
    }
}

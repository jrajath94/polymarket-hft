# API Reference

## Strategy Signals

All strategies emit `StrategySignal` to the executor pipeline:

```rust
pub struct StrategySignal {
    pub strategy: StrategyType,
    pub market_id: String,
    pub token_id: String,
    pub side: Side,           // Buy or Sell
    pub price: Decimal,       // Limit price
    pub size: Decimal,        // USDC amount
    pub urgency: Urgency,     // Normal, High, Critical
    pub timestamp: DateTime<Utc>,
}

pub enum StrategyType {
    SpreadFarming,
    Weather,
    CopyTrade,
    Lp,
    PennyLongshot,
    CustomBot,
}

pub enum Side { Buy, Sell }

pub enum Urgency {
    Normal,    // Queue behind existing orders
    High,      // Skip to front of batch
    Critical,  // Immediate execution, bypass batch
}
```

## Circuit Breaker States

```rust
pub enum CircuitState {
    Closed,    // Normal operation
    Open,      // Trading halted
    HalfOpen,  // Testing with reduced size
}
```

### Triggers

| Trigger | Threshold | Recovery |
|---------|-----------|----------|
| Daily drawdown | `max_daily_drawdown` (default 10%) | Manual reset or next trading day |
| Consecutive losses | `max_consecutive_losses` (default 3) | 1 winning trade in half-open mode |
| WebSocket stale | `ws_stale_threshold_secs` (default 30s) | Auto-recover on reconnect |

### State Transitions

```
Closed --(trigger)--> Open --(cooldown)--> HalfOpen --(success)--> Closed
                                              |
                                              +--(failure)--> Open
```

## Metrics Endpoints

### GET /metrics (Prometheus)

```
# HELP hft_orders_total Total orders placed
# TYPE hft_orders_total counter
hft_orders_total{strategy="spread_farming",side="buy",status="filled"} 142

# HELP hft_order_latency_ms Order-to-wire latency
# TYPE hft_order_latency_ms histogram
hft_order_latency_ms_bucket{strategy="spread_farming",le="1"} 89
hft_order_latency_ms_bucket{strategy="spread_farming",le="5"} 275

# HELP hft_pnl_total Running PnL in USDC
# TYPE hft_pnl_total gauge
hft_pnl_total{strategy="spread_farming"} 12.45

# HELP hft_circuit_breaker_state Circuit breaker state (0=closed, 1=open)
# TYPE hft_circuit_breaker_state gauge
hft_circuit_breaker_state{type="drawdown"} 0
```

### GET /health

```json
{
  "status": "healthy",
  "uptime_secs": 3600,
  "components": {
    "clob_ws": "connected",
    "rtds_ws": "connected",
    "circuit_breaker": "closed",
    "orderbook_age_ms": 150,
    "active_strategies": ["spread_farming", "weather", "copy_trade"],
    "rate_limiter_remaining": 485
  }
}
```

## External API Reference

### Polymarket CLOB API

| Endpoint | Rate Limit | Description |
|----------|-----------|-------------|
| `POST /order` | 500/10s | Place order |
| `DELETE /order/{id}` | 500/10s | Cancel order |
| `GET /book?token_id=` | 1500/10s | Get orderbook |
| `GET /midpoint?token_id=` | 1500/10s | Get midpoint price |
| `WS /ws/market` | N/A | Live book updates |
| `WS /ws/user` | N/A | User order fills |

### Gamma API

| Endpoint | Description |
|----------|-------------|
| `GET /markets` | List all markets |
| `GET /markets?slug=` | Search by slug |
| `GET /events` | List events with markets |

### Data API

| Endpoint | Description |
|----------|-------------|
| `GET /trades` | Recent trades (for copy trading) |
| `GET /markets/{id}` | Market details + volume |

### NOAA API

| Endpoint | Description |
|----------|-------------|
| `GET /points/{lat},{lon}` | Get grid point for coordinates |
| `GET /gridpoints/{office}/{x},{y}/forecast` | 7-day forecast |

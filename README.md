# Polymarket HFT Trading Engine

Production-grade high-frequency trading engine for [Polymarket](https://polymarket.com) prediction markets. Built in Rust with async/await.

## Architecture

```
                          +------------------+
                          |   Config (TOML)  |
                          +--------+---------+
                                   |
                    +--------------v--------------+
                    |        Engine Core          |
                    |  (tokio multi-thread rt)    |
                    +-+---+---+---+---+---+---+--+
                      |   |   |   |   |   |   |
          +-----------+   |   |   |   |   |   +----------+
          |               |   |   |   |   |              |
     +----v----+   +------v-+ | +-v---+---v---+   +------v------+
     |  Auth   |   | CLOB   | | | Gamma| Data |   | Monitoring  |
     | L2/EIP  |   | WS+REST| | |  API | API  |   | Prometheus  |
     +---------+   +--------+ | +------+------+   +-------------+
                               |
                    +----------v-----------+
                    |    Order Book Cache   |
                    |    (DashMap + RwLock) |
                    +----------+-----------+
                               |
              +----------------v----------------+
              |         Strategy Engine          |
              +--+--+--+--+--+--+---------------+
                 |  |  |  |  |  |
    +------------+  |  |  |  |  +-------------+
    |               |  |  |  |                |
+---v---+  +-------v+ | +v--v----+  +--------v---+
|Spread |  |Weather | | |Copy    |  |Penny       |
|Farming|  |Arb     | | |Trade   |  |Longshot    |
+-------+  +--------+ | +--------+  +------------+
                       |
              +--------v--------+
              |  Custom TA Bot  |
              |  RSI/Martingale |
              +-----------------+
                       |
              +--------v--------+
              |    Executor     |
              | Fee Calc + Rate |
              |  Limiter + Batch|
              +--------+--------+
                       |
              +--------v--------+
              |   Risk Engine   |
              | Kelly + Circuit |
              |  Breaker + Pos  |
              +-----------------+
```

## Strategies

| # | Strategy | Description | Status |
|---|----------|-------------|--------|
| 1 | **Spread Farming** | YES+NO < $1 arbitrage on crypto 5m/15m markets | In Progress |
| 2 | **Weather Arb** | NOAA forecast vs Polymarket temperature buckets | In Progress |
| 3 | **Copy Trading** | Mirror whale trades via Data API `/trades` feed | In Progress |
| 4 | **LP Market Making** | Post-only maker orders in stock/event markets | Planned |
| 5 | **Penny Longshot** | Basket of <5c positions (20-50 contracts) | Planned |
| 6 | **Custom TA Bot** | RSI/Martingale/latency arbitrage | Planned |

## Quick Start

### Prerequisites

- Rust 1.75+ (stable)
- Polymarket L2 API credentials
- (Optional) NOAA API key for weather strategy

### Setup

```bash
# Clone
git clone https://github.com/jrajath94/polymarket-hft.git
cd polymarket-hft

# Configure
cp .env.example .env
# Edit .env with your credentials

# Build
cargo build --release

# Run tests
cargo test

# Run benchmarks
cargo bench
```

### Paper Trading

The engine defaults to `development` mode which enables paper trading:

```toml
# config/default.toml
[app]
environment = "development"  # "production" for live trading
```

In development mode:
- Orders are validated but not submitted to the CLOB
- All strategies run against live market data
- PnL tracking and metrics are fully operational
- Circuit breakers and risk limits are enforced

### Configuration

All settings live in `config/default.toml` with env var overrides (prefix `APP__`):

```bash
# Override via environment
export APP__RISK__MAX_DAILY_DRAWDOWN=0.05
export APP__STRATEGIES__SPREAD_FARMING__ENABLED=true
```

## Key Design Decisions

- **`Decimal` everywhere** -- Never use `f64` for prices. `rust_decimal` with string-based serde.
- **TDD throughout** -- Every module has `#[cfg(test)]` blocks. CI enforces 100% test pass.
- **Zero-copy where possible** -- `DashMap` for concurrent orderbook, `crossbeam-channel` for strategy signals.
- **Circuit breakers** -- Daily drawdown limit, consecutive loss limit, WebSocket staleness detection.

## Project Structure

```
src/
  auth/           # L2 HMAC + EIP-712 signing
  clob/           # CLOB REST + WebSocket clients
  gamma/          # Gamma markets API
  data_api/       # Data API (copy trading feed)
  rtds/           # RTDS WebSocket (crypto prices)
  sports_ws/      # Sports API WebSocket
  noaa/           # NOAA weather forecasts
  orderbook/      # In-memory book cache (DashMap)
  executor/       # Fee calc, order builder, batch exec, rate limiter
  risk/           # Kelly sizing, circuit breaker, position tracker
  strategies/     # All 6 strategy implementations
  monitoring/     # Prometheus metrics + health checks
  config.rs       # TOML + env config loader
  error.rs        # Unified error types (thiserror)
  lib.rs          # Module declarations
```

## License

Private. All rights reserved.

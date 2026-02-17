# Architecture

## System Design

The Polymarket HFT engine is a single-binary Rust application built on `tokio` multi-threaded async runtime. It connects to multiple data sources via WebSocket and REST, runs 6 independent strategy loops, and routes orders through a unified execution pipeline with risk controls.

### Core Principles

1. **Event-driven** -- All market data flows through async channels. Strategies react to orderbook updates, not polling.
2. **Decimal precision** -- `rust_decimal::Decimal` for all price/size math. No floating point rounding errors.
3. **Fail-safe** -- Circuit breakers halt trading on drawdown, consecutive losses, or stale data.
4. **Lock-free hot path** -- `DashMap` for orderbook cache, `crossbeam-channel` for signal routing. No `Mutex` on the critical path.

## Data Flow

```
[Polymarket CLOB WS] --> Book Updates --> OrderBook Cache (DashMap)
[RTDS WS]            --> Crypto Prices --> Strategy Engine
[Sports WS]          --> Event Scores  --> Strategy Engine
[NOAA REST]          --> Forecasts     --> Weather Strategy
[Data API REST]      --> Whale Trades  --> Copy Trade Strategy
[Gamma REST]         --> Market Meta   --> All Strategies

Strategy Engine --> StrategySignal --> Executor Pipeline
Executor Pipeline:
  1. FeeCalc      (validate arb exceeds fees)
  2. KellySizer   (position sizing)
  3. RateLimiter  (500 orders/10s token bucket)
  4. OrderBuilder (EIP-712 signed payload)
  5. BatchExecutor(send to CLOB REST)

Risk Engine (parallel):
  - CircuitBreaker monitors drawdown + consecutive losses
  - PositionTracker enforces per-market and portfolio limits
```

## Latency Budget

Target: **< 5ms** order-to-wire for spread farming on local orderbook cache hit.

| Component | Budget | Notes |
|-----------|--------|-------|
| Orderbook lookup | < 100us | DashMap read, no allocation |
| Fee calculation | < 50us | Decimal arithmetic |
| Kelly sizing | < 100us | Single division |
| Rate limit check | < 10us | Atomic counter |
| Order signing (EIP-712) | < 2ms | ECDSA signature |
| HTTP POST to CLOB | < 3ms | Pre-warmed connection pool |
| **Total** | **< 5ms** | Excluding network RTT to PM |

## Module Dependency Graph

```
config --> all modules (injected at startup)
error  --> all modules (unified error type)

auth --> executor (signing)
clob --> orderbook (book updates)
clob --> executor (order submission)
gamma --> strategies (market metadata)
data_api --> strategies/copy_trade
rtds --> strategies (crypto prices)
noaa --> strategies/weather
orderbook --> strategies (cache reads)
strategies --> executor (signals)
executor --> risk (pre-trade checks)
monitoring --> all (metrics collection)
```

## Concurrency Model

- **1 tokio runtime** with multi-threaded scheduler (default thread count = CPU cores)
- **Per-WebSocket tasks** for CLOB, RTDS, Sports feeds (auto-reconnect)
- **Per-strategy tasks** running independent event loops
- **Shared state** via `Arc<DashMap>` (orderbook), `Arc<AtomicU64>` (rate limiter counters)
- **Signal channel** (`crossbeam_channel::bounded`) from strategies to executor

## Configuration Layering

```
config/default.toml   (base)
    |
    v
Environment vars      (APP__ prefix, override TOML)
    |
    v
AppConfig struct       (deserialized, validated)
```

## Error Handling

All errors flow through `AppError` (thiserror-derived enum). Strategies return `Result<StrategySignal>`. The executor catches errors and routes them to:
- Circuit breaker (if trade failure)
- Metrics counter (all errors)
- Tracing span (structured logging)

No `unwrap()` in production paths. `todo!()` only in unimplemented placeholder modules.

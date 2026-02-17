# PROMPT.md — Polymarket HFT Engine: Ralph Loop

## Project Location

/Users/rj/cursorExperiments/polymarket/hft-engine/

## Stack

Rust + tokio monolithic binary. See Cargo.toml.
Docs: ../docs/PRD-TRD-Polymarket-HFT-Strategies.md, ../docs/HFT-Rust-Stack-Spec.md, ../docs/claude_research.md

## TDD Rules (Non-Negotiable)

1. Write `#[cfg(test)]` tests BEFORE implementation code.
2. Every public function has a doc comment.
3. All prices use `rust_decimal::Decimal` (never `f64`).
4. All errors propagate via `anyhow::Result` or `AppError`.
5. Circuit breaker must be checked before ANY order emission.
6. `BatchExecutor` is the ONLY path to place orders.
7. Fee rates are read from config — never hardcoded.
8. Private keys from env vars only (`dotenvy`).

## Current Phase

**Phase 1 — Core Infrastructure**

## Current Task

Implement `src/error.rs` + `src/config.rs` + foundational modules

## API Quick Reference

- CLOB REST: https://clob.polymarket.com
- CLOB WS market: `wss://ws-subscriptions-clob.polymarket.com/ws/market` (ping 10s)
- CLOB WS user: `wss://ws-subscriptions-clob.polymarket.com/ws/user` (auth, ping 10s)
- RTDS WS: `wss://ws-live-data.polymarket.com` (ping 5s)
- Sports WS: `wss://sports-api.polymarket.com/ws`
- Gamma: https://gamma-api.polymarket.com
- Data API: https://data-api.polymarket.com

## L2 Auth

```
POLY_SIGNATURE = HMAC-SHA256(secret, timestamp + METHOD + path + body)
Headers: POLY_ADDRESS, POLY_SIGNATURE, POLY_TIMESTAMP, POLY_API_KEY, POLY_PASSPHRASE
Timestamp expires after 30s; generate fresh per request
```

## EIP-712 Domain

```
name="ClobAuthDomain"
version="1"
chainId=137
CTFExchange=0x4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E
NegRiskExchange=0xC5d563A36AE78145C45a50134d48A1215220f80a
USDC.e=0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174
```

## Risk Hard Stops

- `daily_drawdown > 10%`: halt ALL strategies
- `consecutive_losses > 3`: halt affected strategy
- `ws_stale > 30s`: halt dependent strategies
- Never bypass circuit breaker

## Latency Budget (total: <110ms signal-to-fill)

- WS recv: 1-5ms
- book update: <1ms
- decision: <1ms
- EIP712 sign: 5-20ms
- HTTP POST: 20-80ms

## Rate Limits (Cloudflare queues on 429, don't spam retry)

- `/orders` POST: 500/10s burst
- `/book` GET: 1500/10s
- General: 9000/10s

## Fee Config (load from TOML, never hardcode)

Taker fee markets: 5m/15m crypto, NCAAB, Serie A
Fee formula: `ceil(fee_rate × shares × price × (1 - price))`
20% maker rebate on crypto fee markets

## Output Format (each Ralph Loop iteration)

1. Full file path (absolute)
2. Test file first (test-first TDD)
3. Implementation file
4. `cargo test` output showing tests pass
5. One-line rationale for non-obvious decisions

## Completion Signal

```
<promise>ALL PHASES COMPLETE: cargo test passes, cargo run connects to live CLOB WS,
integration test places and cancels real order, Grafana dashboard live</promise>
```

# Contributing

## Development Workflow

### Branch Strategy

- `main` -- Always deployable. Protected with required reviews + CI.
- `feat/*` -- New features and strategies.
- `fix/*` -- Bug fixes.
- `refactor/*` -- Code improvements without behavior changes.

### Pull Request Process

1. Create a feature branch from `main`
2. Write tests FIRST (TDD)
3. Implement until tests pass
4. Run full test suite: `cargo test`
5. Run clippy: `cargo clippy -- -D warnings`
6. Run format check: `cargo fmt -- --check`
7. Open PR against `main`
8. PR requires 1 approval + all CI checks passing

### Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(spread-farming): add YES+NO price validation
fix(rate-limiter): correct token bucket refill timing
test(circuit-breaker): add drawdown threshold edge cases
refactor(executor): extract fee calc into separate module
docs(readme): update architecture diagram
```

## TDD Rules

**This project follows strict TDD. Every PR must include tests.**

### Test Structure

```rust
// In each module file
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_specific_behavior() {
        // Arrange
        let input = ...;
        // Act
        let result = function_under_test(input);
        // Assert
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_async_behavior() {
        // For async functions
    }
}
```

### Test Categories

| Type | Location | Run Command |
|------|----------|-------------|
| Unit | `src/**/*.rs` (`#[cfg(test)]`) | `cargo test` |
| Integration | `tests/integration/` | `cargo test --test '*'` |
| Benchmarks | `benches/` | `cargo bench` |

### Coverage Requirements

- New code must have test coverage for all public functions
- Edge cases (zero values, max values, error paths) must be tested
- Decimal precision tests: verify no `f64` rounding issues

### What to Test

- **Fee calculations** -- Exact decimal arithmetic against known values
- **Kelly sizing** -- Boundary conditions (edge=0, negative edge)
- **Circuit breakers** -- State transitions (closed -> open -> half-open)
- **Rate limiter** -- Token refill timing, burst behavior
- **Order builder** -- EIP-712 payload structure, signature validity
- **Strategy signals** -- Correct signal generation from market data fixtures

## Code Style

### Enforced by CI

- `cargo fmt` -- Rustfmt with default settings
- `cargo clippy -- -D warnings` -- All clippy lints as errors

### Conventions

- Use `Decimal` (not `f64`) for any price, size, or fee value
- Use `thiserror` for error types, `anyhow` only in tests/scripts
- Prefer `Arc<DashMap>` over `Arc<Mutex<HashMap>>`
- Use `tracing` (not `println!` or `log`) for all logging
- Keep functions < 50 lines. Extract helpers if longer.
- No `unwrap()` in `src/`. Use `?` or explicit error handling.

## Running Locally

```bash
# Full test suite
cargo test

# Single module
cargo test --lib config

# With logging
RUST_LOG=debug cargo test -- --nocapture

# Clippy
cargo clippy -- -D warnings

# Format
cargo fmt

# Benchmarks
cargo bench

# Security audit
cargo audit
```

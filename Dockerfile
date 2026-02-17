# ── Build Stage ──────────────────────────────────────────────────────
FROM rust:1.77-bookworm AS builder

WORKDIR /app

# Cache dependencies by copying manifests first
COPY Cargo.toml Cargo.lock ./

# Create a dummy main to build deps
RUN mkdir src && echo "fn main() {}" > src/main.rs && \
    mkdir -p src/bin && echo "fn main() {}" > src/bin/backtest.rs && \
    mkdir benches && echo "fn main() {}" > benches/hot_path.rs

RUN cargo build --release 2>/dev/null || true
RUN rm -rf src benches

# Copy actual source
COPY src/ src/
COPY benches/ benches/
COPY config/ config/

# Build the real binary (touch to invalidate cache)
RUN touch src/main.rs && cargo build --release --bin polymarket-hft
RUN touch src/bin/backtest.rs && cargo build --release --bin backtest

# ── Runtime Stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for security
RUN useradd --create-home --shell /bin/bash hft
USER hft
WORKDIR /home/hft

# Copy binaries from build stage
COPY --from=builder /app/target/release/polymarket-hft ./polymarket-hft
COPY --from=builder /app/target/release/backtest ./backtest
COPY --from=builder /app/config/ ./config/

# Prometheus metrics on 9090, API/health on 3333
EXPOSE 9090 3333

# Health check hits the /health endpoint
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3333/health || exit 1

ENV APP_CONFIG_PATH=/home/hft/config/default.toml
ENV RUST_LOG=info

ENTRYPOINT ["./polymarket-hft"]

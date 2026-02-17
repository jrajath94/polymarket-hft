# Deployment

## Docker

### Build

```bash
# Build optimized release image
docker build -t polymarket-hft:latest .

# Run with env file
docker run --env-file .env -p 9090:9090 polymarket-hft:latest
```

### Docker Compose

```bash
# Start engine + Prometheus + Grafana
docker compose up -d

# Check logs
docker compose logs -f hft-engine

# Stop
docker compose down
```

### Services

| Service | Port | Description |
|---------|------|-------------|
| hft-engine | 9090 | Prometheus metrics endpoint |
| prometheus | 9091 | Prometheus UI |
| grafana | 3000 | Grafana dashboards |

## OCI Deployment

### Prerequisites

- OCI instance (recommended: VM.Standard.E4.Flex, 4 OCPU / 16 GB)
- Docker + Docker Compose installed
- SSH access configured

### Deploy

```bash
# SSH to instance
ssh -i ~/.ssh/id_rsa ubuntu@<OCI_IP>

# Clone repo
git clone https://github.com/jrajath94/polymarket-hft.git
cd polymarket-hft

# Configure
cp .env.example .env
vim .env  # Add API credentials

# Start
docker compose up -d

# Verify
curl http://localhost:9090/metrics
```

### Updates

```bash
cd /home/ubuntu/polymarket-hft
git pull origin main
docker compose build --no-cache hft-engine
docker compose up -d hft-engine
```

## Monitoring

### Prometheus Metrics

The engine exposes metrics at `:9090/metrics`:

- `hft_orders_total{strategy,side,status}` -- Order count by strategy
- `hft_order_latency_ms{strategy}` -- Order-to-wire latency histogram
- `hft_pnl_total{strategy}` -- Running PnL by strategy
- `hft_circuit_breaker_state{type}` -- Circuit breaker states (0=closed, 1=open)
- `hft_ws_connected{feed}` -- WebSocket connection status
- `hft_orderbook_staleness_ms{market}` -- Time since last book update

### Health Check

```bash
curl http://localhost:9090/health
```

### Alerts

Configure Prometheus alerting rules for:
- Circuit breaker open for > 5 minutes
- WebSocket disconnected for > 30 seconds
- Order error rate > 5% over 1 minute
- Daily PnL below -5% threshold

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `PRIVATE_KEY` | Yes | Ethereum private key (L1 wallet) |
| `CLOB_API_KEY` | Yes | Polymarket CLOB API key |
| `CLOB_API_SECRET` | Yes | CLOB API secret |
| `CLOB_API_PASSPHRASE` | Yes | CLOB API passphrase |
| `NOAA_API_KEY` | No | NOAA weather API key (weather strategy) |
| `APP__APP__ENVIRONMENT` | No | Override environment (development/production) |
| `RUST_LOG` | No | Log level filter (default: info) |

## Production Checklist

- [ ] Set `APP__APP__ENVIRONMENT=production`
- [ ] Verify all API credentials in `.env`
- [ ] Confirm circuit breaker thresholds in `config/default.toml`
- [ ] Set conservative `max_daily_drawdown` (start with 0.02)
- [ ] Enable only tested strategies initially
- [ ] Verify Prometheus + alerting is configured
- [ ] Test paper trading for 24h before going live
- [ ] Set up log rotation

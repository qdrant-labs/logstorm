# emitter

A synthetic log generator written in Rust. Simulates multiple microservices producing structured log entries at configurable rates, embeds them via OpenAI, and flushes batches to one or more pluggable sinks.

## How it works

1. **Message pool** — On startup, a pool of unique log messages is generated combinatorially and embedded in bulk using OpenAI's `text-embedding-3-small` model.
2. **Emit** — Per-service async tasks sample from the pool at the configured rate, producing `LogEntry` structs (id, timestamp, service, level, message, embedding).
3. **Buffer** — Entries are collected into a shared buffer and flushed to all configured sinks when the buffer fills or a flush interval elapses.

## Sinks

Sinks are enabled via Cargo feature flags:

| Sink | Feature | Description |
|------|---------|-------------|
| Stdout | *(always available)* | Logs entries via `tracing` |
| Qdrant | `qdrant` | Upserts with dense + sparse (BM25) vectors |
| Elasticsearch | `elasticsearch` | Bulk index with dense vectors + BM25 text field |
| pgvector | `pgvector` | Batch insert via `UNNEST` with `vector` column |
| Dashboard | `dashboard` | WebSocket server for live log streaming |

## Usage

```bash
# run with default config
cargo run --release --features "qdrant,elasticsearch,dashboard"

# run for a specific duration
cargo run --release --features "qdrant,elasticsearch" -- --duration-secs 120

# custom config file
cargo run --release --features "qdrant" -- -c my_config.yaml
```

## Configuration

See `config.yaml`. Environment variables are expanded using `${VAR_NAME}` syntax.

```yaml
buffer_size: 1000
flush_interval_ms: 3000
run_duration_secs: 0          # 0 = run indefinitely
message_pool_size: 1000

embedding:
  api_key: ${OPENAI_API_KEY}
  model: text-embedding-3-small
  dimensions: 1536

sinks:
  - type: qdrant
    url: ${QDRANT_URL}
    collection_name: logs
  - type: elasticsearch
    url: ${ELASTIC_URL}
    user: ${ELASTIC_USER}
    password: ${ELASTIC_PASSWORD}
    index_name: logs
  - type: pgvector
    host: ${PGVECTOR_HOST}
    user: ${PGVECTOR_USER}
    password: ${PGVECTOR_PASSWORD}
    table_name: logs
  - type: dashboard
    port: 3000

services:
  - name: api-gateway
    rate_per_sec: 30.0
    level_weights:
      debug: 0.1
      info: 0.7
      warn: 0.15
      error: 0.05
```
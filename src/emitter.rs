use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::config::{LogLevelWeights, ServiceConfig};
use crate::log_entry::{LogEntry, LogLevel};

// ---------------------------------------------------------------------------
// Slot-based combinatorial message generation
// 20 items per slot × 4 patterns × 20^4..20^5 = ~640K–12.8M unique messages
// ---------------------------------------------------------------------------

const COMPONENTS: &[&str] = &[
    "ConnectionPool",
    "QueryExecutor",
    "AuthManager",
    "CacheLayer",
    "LoadBalancer",
    "RateLimiter",
    "SchemaValidator",
    "EventBus",
    "HealthMonitor",
    "SessionStore",
    "CircuitBreaker",
    "JobScheduler",
    "ReplicaManager",
    "PartitionConsumer",
    "TLSHandler",
    "GarbageCollector",
    "DiskMonitor",
    "DeadLetterQueue",
    "IdempotencyFilter",
    "TraceContext",
];

const ACTIONS: &[&str] = &[
    "detected threshold breach",
    "completed successfully",
    "failed after retries",
    "initiated graceful recovery",
    "rejected invalid request",
    "triggered rebalance",
    "exceeded soft limit",
    "evicted stale entry",
    "propagated context",
    "acquired resource handle",
    "flushed pending writes",
    "rotated credentials",
    "promoted fallback path",
    "checkpointed at offset",
    "enqueued background task",
    "resolved after backoff",
    "timed out waiting",
    "received reset signal",
    "applied migration",
    "scheduled maintenance",
];

const METRICS: &[&str] = &[
    "latency=2340ms",
    "count=184302",
    "ratio=0.94",
    "depth=500",
    "attempt=3/5",
    "usage=85%",
    "lag=500ms",
    "size=1.2MB",
    "rate=120/s",
    "ttl=3600s",
    "connections=980/1024",
    "duration=30s",
    "retries=3",
    "offset=48291",
    "queue_depth=1247",
    "p99=450ms",
    "batch_size=500",
    "memory=2.4GB",
    "iops=12000",
    "threads=48",
];

const TARGETS: &[&str] = &[
    "on orders table",
    "for payments-api",
    "from upstream host",
    "in consumer group",
    "on /data volume",
    "for tenant af923c",
    "to downstream service",
    "across service boundary",
    "in container cgroup",
    "from replica node-3",
    "on topic order.completed",
    "for client session",
    "in write-ahead log",
    "on port 8443",
    "from discovery endpoint",
    "in ring buffer",
    "for user session",
    "on primary shard",
    "to dead letter queue",
    "from environment config",
];

const CONTEXTS: &[&str] = &[
    "(retrying)",
    "(non-blocking)",
    "(scheduled)",
    "(cached response)",
    "(dark_launch=true)",
    "(read-only)",
    "(best-effort)",
    "(idempotent)",
    "(correlation_id=missing)",
    "(circuit=open)",
    "(degraded mode)",
    "(cold start)",
    "(warm path)",
    "(fallback)",
    "(async)",
    "(batched)",
    "(compressed)",
    "(encrypted)",
    "(sampled)",
    "(throttled)",
];

fn pick<'a>(list: &[&'a str], rng: &mut impl Rng) -> &'a str {
    list[rng.gen_range(0..list.len())]
}

pub fn generate_message(rng: &mut impl Rng) -> String {
    let pattern = rng.gen_range(0..4u8);
    match pattern {
        0 => format!(
            "{}: {} {} {}",
            pick(COMPONENTS, rng),
            pick(ACTIONS, rng),
            pick(TARGETS, rng),
            pick(CONTEXTS, rng),
        ),
        1 => format!(
            "{}: {} [{}] {}",
            pick(COMPONENTS, rng),
            pick(ACTIONS, rng),
            pick(METRICS, rng),
            pick(TARGETS, rng),
        ),
        2 => format!(
            "{}: {} [{}]",
            pick(COMPONENTS, rng),
            pick(ACTIONS, rng),
            pick(METRICS, rng),
        ),
        _ => format!(
            "{}: {} {} [{}] {}",
            pick(COMPONENTS, rng),
            pick(ACTIONS, rng),
            pick(TARGETS, rng),
            pick(METRICS, rng),
            pick(CONTEXTS, rng),
        ),
    }
}

/// Pre-generate a pool of unique messages for embedding at startup.
pub fn build_message_pool(rng: &mut impl Rng, size: usize) -> Vec<String> {
    let mut pool = std::collections::HashSet::with_capacity(size);
    while pool.len() < size {
        pool.insert(generate_message(rng));
    }
    pool.into_iter().collect()
}

/// Add small noise to an embedding to prevent degenerate HNSW clusters
/// from duplicate vectors while preserving semantic locality.
fn jitter_embedding(embedding: &[f32], rng: &mut impl Rng, scale: f32) -> Vec<f32> {
    embedding
        .iter()
        .map(|&v| {
            let noise = rng.gen_range(-1.0f32..1.0) * scale * v.abs().max(0.01);
            v + noise
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Log generation + emission
// ---------------------------------------------------------------------------

pub fn generate_log(
    service: &ServiceConfig,
    rng: &mut impl Rng,
    pool: &[String],
    embeddings: &HashMap<String, Vec<f32>>,
) -> LogEntry {
    let level = pick_level(&service.level_weights, rng);
    let message = &pool[rng.gen_range(0..pool.len())];
    let base_embedding = embeddings.get(message).cloned().unwrap_or_default();
    let embedding = jitter_embedding(&base_embedding, rng, 0.01);

    LogEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        service: service.name.clone(),
        level,
        message: message.clone(),
        embedding,
    }
}

pub async fn emit_logs(
    service: ServiceConfig,
    tx: mpsc::Sender<LogEntry>,
    duration: Duration,
    pool: Arc<Vec<String>>,
    embeddings: Arc<HashMap<String, Vec<f32>>>,
) {
    let mut rng = StdRng::from_entropy();
    let start = Instant::now();
    let mean_interval_ms = 1000.0 / service.rate_per_sec;

    while duration.is_zero() || start.elapsed() < duration {
        let log = generate_log(&service, &mut rng, &pool, &embeddings);
        if tx.send(log).await.is_err() {
            break;
        }

        // Exponential inter-arrival time (Poisson process)
        let u: f64 = rng.gen_range(f64::EPSILON..1.0);
        let delay_ms = (-mean_interval_ms * u.ln()) as u64;
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
}

fn pick_level(weights: &LogLevelWeights, rng: &mut impl Rng) -> LogLevel {
    let total = weights.debug + weights.info + weights.warn + weights.error;
    let roll: f64 = rng.gen_range(0.0..total);

    if roll < weights.debug {
        LogLevel::Debug
    } else if roll < weights.debug + weights.info {
        LogLevel::Info
    } else if roll < weights.debug + weights.info + weights.warn {
        LogLevel::Warn
    } else {
        LogLevel::Error
    }
}

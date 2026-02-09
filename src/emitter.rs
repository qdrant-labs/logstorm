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

pub const MESSAGES: &[&str] = &[
    // General operations
    "Request processed successfully",
    "Connection established to downstream service",
    "Cache miss, fetching from database",
    "Rate limit threshold approaching",
    "Successfully processed batch of records",
    "Health check passed",
    "Configuration reloaded from environment",
    "Scheduled job completed without errors",
    "Graceful shutdown initiated",
    "Service registered with discovery endpoint",
    "Warm-up sequence completed, accepting traffic",
    "Feature flag evaluated: dark_launch=true",
    // Database / storage
    "Failed to connect to database, retrying",
    "Connection pool acquired new handle from database",
    "Slow query detected: SELECT took 2340ms",
    "Database replica lag exceeds 500ms threshold",
    "Transaction committed after 3 retries",
    "Migration applied: 20240815_add_index_users_email",
    "Dead tuple ratio above threshold, scheduling vacuum",
    "Write-ahead log flush latency spike: 120ms",
    "Row lock contention detected on orders table",
    "Read replica promoted to primary",
    // Authentication / authorization
    "Authentication token expired for user session",
    "JWT signature verification failed",
    "OAuth refresh token rotation completed",
    "RBAC policy denied access to resource",
    "API key rate limit exceeded for tenant",
    "Session invalidated due to concurrent login",
    "MFA challenge issued, awaiting response",
    "CORS preflight rejected from unknown origin",
    // Network / HTTP
    "Request timeout after 30s waiting for response",
    "Outbound connection pool exhausted",
    "Retry attempt 3/5 for upstream request",
    "TLS handshake failed: certificate expired",
    "DNS resolution took 850ms for upstream host",
    "Circuit breaker tripped for payments-api",
    "Load balancer health check returned 503",
    "gRPC stream reset by peer: GOAWAY received",
    "WebSocket connection upgraded successfully",
    "HTTP/2 PING timeout, closing connection",
    "Proxy returned 429 Too Many Requests",
    "Connection refused: upstream not listening on port 8443",
    // Infrastructure / resources
    "Disk usage above 85% threshold",
    "Memory allocation spike detected",
    "CPU throttling detected in container cgroup",
    "Garbage collection pause exceeded 200ms",
    "File descriptor limit approaching: 980/1024",
    "Swap usage detected, possible memory pressure",
    "Container OOM kill risk: memory at 94%",
    "Disk IOPS saturated on /data volume",
    "Network interface packet drop rate increasing",
    "Kernel ring buffer overflow detected",
    // Application logic
    "Unexpected null value in response payload",
    "Idempotency key collision detected, returning cached response",
    "Deduplication filter rejected duplicate event",
    "Message deserialization failed: unexpected field 'metadata'",
    "Payload size exceeds 1MB soft limit",
    "Event published to topic: order.completed",
    "Dead letter queue depth exceeds 500 messages",
    "Schema validation failed: missing required field 'amount'",
    "Partition rebalance triggered for consumer group",
    "Batch processing checkpoint saved at offset 184302",
    "Stale cache entry evicted after TTL expiry",
    "Background job enqueued: generate_monthly_report",
    "Rate limiter bucket refilled for client_id=af923c",
    "Trace context propagated across service boundary",
    "Correlation ID not found in request headers, generating new",
];

pub fn generate_log(
    service: &ServiceConfig,
    rng: &mut impl Rng,
    embeddings: &HashMap<String, Vec<f32>>,
) -> LogEntry {
    let level = pick_level(&service.level_weights, rng);
    let message = MESSAGES[rng.gen_range(0..MESSAGES.len())].to_string();
    let embedding = embeddings.get(&message).cloned().unwrap_or_default();

    LogEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        service: service.name.clone(),
        level,
        message,
        embedding,
    }
}

pub async fn emit_logs(
    service: ServiceConfig,
    tx: mpsc::Sender<LogEntry>,
    duration: Duration,
    embeddings: Arc<HashMap<String, Vec<f32>>>,
) {
    let mut rng = StdRng::from_entropy();
    let start = Instant::now();
    let mean_interval_ms = 1000.0 / service.rate_per_sec;

    while duration.is_zero() || start.elapsed() < duration {
        let log = generate_log(&service, &mut rng, &embeddings);
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

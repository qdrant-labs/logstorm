use crate::config::{LogLevelWeights, ServiceConfig};
use crate::log_entry::{LogEntry, LogLevel};
use chrono::Utc;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use uuid::Uuid;

const MESSAGES: &[&str] = &[
    "Request processed successfully",
    "Connection established to downstream service",
    "Cache miss, fetching from database",
    "Rate limit threshold approaching",
    "Failed to connect to database, retrying",
    "Authentication token expired for user session",
    "Request timeout after 30s waiting for response",
    "Disk usage above 85% threshold",
    "Successfully processed batch of records",
    "Unexpected null value in response payload",
    "Health check passed",
    "Configuration reloaded from environment",
    "Outbound connection pool exhausted",
    "Retry attempt 3/5 for upstream request",
    "Memory allocation spike detected",
];

pub fn generate_log(service: &ServiceConfig, rng: &mut impl Rng) -> LogEntry {
    let level = pick_level(&service.level_weights, rng);
    let message = MESSAGES[rng.gen_range(0..MESSAGES.len())].to_string();

    LogEntry {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now(),
        service: service.name.clone(),
        level,
        message,
    }
}

pub async fn emit_logs(service: ServiceConfig, tx: mpsc::Sender<LogEntry>, duration: Duration) {
    let mut rng = StdRng::from_entropy();
    let start = Instant::now();
    let mean_interval_ms = 1000.0 / service.rate_per_sec;

    while start.elapsed() < duration {
        let log = generate_log(&service, &mut rng);
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
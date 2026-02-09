use emitter::buffer::Buffer;
use emitter::config::EmitterConfig;
use emitter::emitter::emit_logs;
use emitter::sink::{Sink, StdoutSink};
use std::time::Duration;
use tokio::sync::mpsc;

use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

fn load_config() -> EmitterConfig {
    match std::fs::read_to_string("config.yaml") {
        Ok(contents) => serde_yaml::from_str(&contents).expect("Invalid config.yaml"),
        Err(_) => {
            info!("No config.yaml found, using defaults");
            EmitterConfig::default()
        }
    }
}

#[tokio::main]
async fn main() {
    let config = load_config();
    let duration = Duration::from_secs(config.run_duration_secs);

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    info!(
        "Starting emitter: {} services, {}s duration, buffer={}",
        config.services.len(),
        config.run_duration_secs,
        config.buffer_size,
    );

    let (tx, rx) = mpsc::channel(10_000);

    // Spawn one emitter task per service
    for service in &config.services {
        let tx = tx.clone();
        let service = service.clone();
        tokio::spawn(async move {
            emit_logs(service, tx, duration).await;
        });
    }
    drop(tx); // Drop original sender so channel closes when emitters finish

    // Run the buffer with a stdout sink (swap for ES/Qdrant later)
    let sinks: Vec<Box<dyn Sink>> = vec![Box::new(StdoutSink)];
    let mut buffer = Buffer::new(
        rx,
        sinks,
        config.buffer_size,
        Duration::from_millis(config.flush_interval_ms),
    );
    buffer.run().await;

    info!("Done.");
}

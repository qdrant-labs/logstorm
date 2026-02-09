use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

use emitter::buffer::Buffer;
use emitter::config::{EmitterConfig, SinkConfig};
use emitter::embedding::EmbeddingService;
use emitter::emitter::{emit_logs, MESSAGES};
use emitter::sink::{Sink, StdoutSink};

/// Expand `${VAR_NAME}` patterns in a string with environment variable values.
/// Unknown vars become empty strings.
fn expand_env_vars(input: &str) -> String {
    let mut result = input.to_string();
    while let Some(start) = result.find("${") {
        let Some(end) = result[start..].find('}') else {
            break;
        };
        let var_name = &result[start + 2..start + end];
        let value = std::env::var(var_name).unwrap_or_default();
        result = format!(
            "{}{}{}",
            &result[..start],
            value,
            &result[start + end + 1..]
        );
    }
    result
}

fn load_config() -> EmitterConfig {
    match std::fs::read_to_string("config.yaml") {
        Ok(contents) => {
            let expanded = expand_env_vars(&contents);
            serde_yaml::from_str(&expanded).expect("Invalid config.yaml")
        }
        Err(_) => {
            info!("No config.yaml found, using defaults");
            EmitterConfig::default()
        }
    }
}

async fn build_sinks(sink_configs: &[SinkConfig]) -> Vec<Box<dyn Sink>> {
    sink_configs
        .iter()
        .filter_map(|cfg| match cfg {
            SinkConfig::Stdout => Some(Box::new(StdoutSink) as Box<dyn Sink>),
            #[cfg(feature = "qdrant")]
            SinkConfig::Qdrant(_qdrant_cfg) => {
                // TODO: construct QdrantSink from config
                info!("Qdrant sink configured but not yet implemented, skipping");
                None
            }
            #[cfg(feature = "elasticsearch")]
            SinkConfig::ElasticSearch(_es_cfg) => {
                // TODO: construct ElasticSearchSink from config
                info!("Elasticsearch sink configured but not yet implemented, skipping");
                None
            }
        })
        .collect()
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let config = load_config();
    let duration = Duration::from_secs(config.run_duration_secs);

    info!(
        "Starting emitter: {} services, {} sinks, {}s duration, buffer={}",
        config.services.len(),
        config.sinks.len(),
        config.run_duration_secs,
        config.buffer_size,
    );

    // Embed all messages once at startup
    let embedding_service = EmbeddingService::from_config(config.embedding.clone());
    let embeddings = Arc::new(
        embedding_service
            .embed_all(MESSAGES)
            .await
            .expect("Failed to generate embeddings"),
    );

    let sinks = build_sinks(&config.sinks).await;
    let (tx, rx) = mpsc::channel(10_000);

    for service in &config.services {
        let tx = tx.clone();
        let service = service.clone();
        let embeddings = Arc::clone(&embeddings);
        tokio::spawn(async move {
            emit_logs(service, tx, duration, embeddings).await;
        });
    }
    drop(tx);

    let mut buffer = Buffer::new(
        rx,
        sinks,
        config.buffer_size,
        Duration::from_millis(config.flush_interval_ms),
    );
    buffer.run().await;

    info!("Done.");
}
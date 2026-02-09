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

#[allow(unused_variables)]
async fn build_sinks(sink_configs: &[SinkConfig], embedding_dim: usize) -> Vec<Box<dyn Sink>> {
    let mut sinks: Vec<Box<dyn Sink>> = Vec::new();
    for cfg in sink_configs {
        match cfg {
            SinkConfig::Stdout {} => {
                sinks.push(Box::new(StdoutSink));
            }
            #[cfg(feature = "qdrant")]
            SinkConfig::Qdrant(qdrant_cfg) => {
                use emitter::sink::qdrant::QdrantSink;
                let qdrant_sink = QdrantSink::from_config(qdrant_cfg.to_owned(), embedding_dim).await;
                info!("Qdrant sink configured for collection '{}'", qdrant_cfg.collection_name);
                sinks.push(Box::new(qdrant_sink));
            }
            #[cfg(feature = "elasticsearch")]
            SinkConfig::ElasticSearch(es_cfg) => {
                use emitter::sink::elasticsearch::ElasticSearchSink;
                let es_sink = ElasticSearchSink::from_config(es_cfg.to_owned(), embedding_dim).await;
                info!("Elasticsearch sink configured for index '{}'", es_cfg.index_name);
                sinks.push(Box::new(es_sink));
            }
            #[cfg(feature = "dashboard")]
            SinkConfig::Dashboard(dashboard_cfg) => {
                use emitter::sink::dashboard::{DashboardSink, start_dashboard_server};
                let (tx, _rx) = tokio::sync::broadcast::channel(100);
                tokio::spawn(start_dashboard_server(dashboard_cfg.port, tx.clone()));
                info!("Dashboard sink configured on port {}", dashboard_cfg.port);
                sinks.push(Box::new(DashboardSink::new(tx)));
            }
        }
    }
    sinks
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

    // Embed all messages once at startup (fastembed is sync, so use spawn_blocking)
    let embedding_config = config.embedding.clone();
    let (embeddings, embedding_dim) = tokio::task::spawn_blocking(move || {
        let service = EmbeddingService::from_config(&embedding_config);
        let dim = service.dimension();
        let map = service.embed_all(MESSAGES).expect("Failed to generate embeddings");
        (map, dim)
    })
    .await
    .expect("Embedding task panicked");
    let embeddings = Arc::new(embeddings);

    info!("Embedding dimension: {}", embedding_dim);
    let sinks = build_sinks(&config.sinks, embedding_dim).await;
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

    info!("Emitter running for {} seconds...", config.run_duration_secs);
    buffer.run().await;

    info!("Done.");
}
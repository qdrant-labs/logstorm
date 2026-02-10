use serde::{Deserialize, Serialize};

#[cfg(feature = "dashboard")]
use crate::sink::dashboard::DashboardConfig;
#[cfg(feature = "elasticsearch")]
use crate::sink::elasticsearch::ElasticSearchConfig;
#[cfg(feature = "pgvector")]
use crate::sink::pgvector::PgvectorConfig;
#[cfg(feature = "qdrant")]
use crate::sink::qdrant::QdrantConfig;

fn default_message_pool_size() -> usize {
    10_000
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_embedding_dimensions() -> u32 {
    1536
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub api_key: String,
    #[serde(default = "default_embedding_model")]
    pub model: String,
    #[serde(default = "default_embedding_dimensions")]
    pub dimensions: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SinkConfig {
    Stdout {},
    #[cfg(feature = "qdrant")]
    Qdrant(QdrantConfig),
    #[cfg(feature = "elasticsearch")]
    #[serde(rename = "elasticsearch")]
    ElasticSearch(ElasticSearchConfig),
    #[cfg(feature = "pgvector")]
    Pgvector(PgvectorConfig),
    #[cfg(feature = "dashboard")]
    Dashboard(DashboardConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitterConfig {
    pub buffer_size: usize,
    pub flush_interval_ms: u64,
    pub run_duration_secs: u64,
    #[serde(default = "default_message_pool_size")]
    pub message_pool_size: usize,
    pub services: Vec<ServiceConfig>,
    pub sinks: Vec<SinkConfig>,
    pub embedding: EmbeddingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub rate_per_sec: f64,
    pub level_weights: LogLevelWeights,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLevelWeights {
    pub debug: f64,
    pub info: f64,
    pub warn: f64,
    pub error: f64,
}

impl Default for EmitterConfig {
    fn default() -> Self {
        Self {
            buffer_size: 1000,
            flush_interval_ms: 5000,
            run_duration_secs: 30,
            message_pool_size: default_message_pool_size(),
            sinks: vec![SinkConfig::Stdout {}],
            embedding: EmbeddingConfig {
                api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                model: default_embedding_model(),
                dimensions: default_embedding_dimensions(),
            },
            services: vec![
                ServiceConfig {
                    name: "api-gateway".into(),
                    rate_per_sec: 100.0,
                    level_weights: LogLevelWeights {
                        debug: 0.1,
                        info: 0.7,
                        warn: 0.15,
                        error: 0.05,
                    },
                },
                ServiceConfig {
                    name: "auth-service".into(),
                    rate_per_sec: 50.0,
                    level_weights: LogLevelWeights {
                        debug: 0.05,
                        info: 0.6,
                        warn: 0.2,
                        error: 0.15,
                    },
                },
                ServiceConfig {
                    name: "payment-service".into(),
                    rate_per_sec: 30.0,
                    level_weights: LogLevelWeights {
                        debug: 0.05,
                        info: 0.5,
                        warn: 0.25,
                        error: 0.2,
                    },
                },
                ServiceConfig {
                    name: "user-service".into(),
                    rate_per_sec: 40.0,
                    level_weights: LogLevelWeights {
                        debug: 0.1,
                        info: 0.65,
                        warn: 0.15,
                        error: 0.1,
                    },
                },
            ],
        }
    }
}

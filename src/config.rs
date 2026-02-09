use serde::{Deserialize, Serialize};

#[cfg(feature = "dashboard")]
use crate::sink::dashboard::DashboardConfig;
#[cfg(feature = "elasticsearch")]
use crate::sink::elasticsearch::ElasticSearchConfig;
#[cfg(feature = "qdrant")]
use crate::sink::qdrant::QdrantConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub api_key: String,
    pub model: String,
    pub dimensions: Option<u32>,
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
    #[cfg(feature = "dashboard")]
    Dashboard(DashboardConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitterConfig {
    pub buffer_size: usize,
    pub flush_interval_ms: u64,
    pub run_duration_secs: u64,
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
            sinks: vec![SinkConfig::Stdout {}],
            embedding: EmbeddingConfig {
                api_key: std::env::var("OPENAI_API_KEY").unwrap_or_default(),
                model: "text-embedding-3-small".into(),
                dimensions: Some(1536),
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

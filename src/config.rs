use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitterConfig {
    pub buffer_size: usize,
    pub flush_interval_ms: u64,
    pub run_duration_secs: u64,
    pub services: Vec<ServiceConfig>,
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
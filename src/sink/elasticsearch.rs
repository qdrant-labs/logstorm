use serde::{Deserialize, Serialize};

fn default_index_name() -> String {
    "logs".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElasticSearchConfig {
    pub url: String,
    pub api_key: Option<String>,
    #[serde(default = "default_index_name")]
    pub index_name: String,
}
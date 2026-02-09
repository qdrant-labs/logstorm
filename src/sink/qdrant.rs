use serde::{Deserialize, Serialize};

fn default_collection_name() -> String {
    "logs".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantConfig {
    pub url: String,
    pub api_key: Option<String>,
    #[serde(default = "default_collection_name")]
    pub collection_name: String,
}
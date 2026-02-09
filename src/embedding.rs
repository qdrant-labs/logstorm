use std::collections::HashMap;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use tracing::info;

use crate::config::EmbeddingConfig;

pub struct EmbeddingService {
    model: TextEmbedding,
}

impl EmbeddingService {
    pub fn from_config(config: &EmbeddingConfig) -> Self {
        let model = parse_model(&config.model)
            .unwrap_or_else(|e| panic!("{e}"));
        let embedding = TextEmbedding::try_new(
            InitOptions::new(model).with_show_download_progress(true),
        )
        .expect("Failed to load embedding model");
        Self { model: embedding }
    }

    pub fn dimension(&self) -> usize {
        self.model
            .embed(vec!["test"], None)
            .map(|v| v.first().map(|e| e.len()).unwrap_or(0))
            .unwrap_or(0)
    }

    /// Embed all messages in a single batch call. Returns a map from message text
    /// to its embedding vector. Call this once at startup.
    pub fn embed_all(
        &self,
        messages: &[&str],
    ) -> Result<HashMap<String, Vec<f32>>, Box<dyn std::error::Error + Send + Sync>> {
        info!("Embedding {} messages with fastembed", messages.len());

        let input: Vec<String> = messages.iter().map(|s| s.to_string()).collect();
        let embeddings = self.model.embed(input, None)?;

        let mut map = HashMap::with_capacity(messages.len());
        for (i, embedding) in embeddings.into_iter().enumerate() {
            map.insert(messages[i].to_string(), embedding);
        }

        info!("Embedded {} messages successfully", map.len());
        Ok(map)
    }
}

fn parse_model(name: &str) -> Result<EmbeddingModel, String> {
    match name {
        "BAAI/bge-small-en-v1.5" | "bge-small-en-v1.5" => Ok(EmbeddingModel::BGESmallENV15),
        "BAAI/bge-base-en-v1.5" | "bge-base-en-v1.5" => Ok(EmbeddingModel::BGEBaseENV15),
        "BAAI/bge-large-en-v1.5" | "bge-large-en-v1.5" => Ok(EmbeddingModel::BGELargeENV15),
        "sentence-transformers/all-MiniLM-L6-v2" | "all-MiniLM-L6-v2" => {
            Ok(EmbeddingModel::AllMiniLML6V2)
        }
        "sentence-transformers/all-MiniLM-L12-v2" | "all-MiniLM-L12-v2" => {
            Ok(EmbeddingModel::AllMiniLML12V2)
        }
        _ => Err(format!(
            "Unknown embedding model: '{}'. Supported: bge-small-en-v1.5, bge-base-en-v1.5, bge-large-en-v1.5, all-MiniLM-L6-v2, all-MiniLM-L12-v2",
            name
        )),
    }
}
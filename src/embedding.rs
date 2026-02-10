use std::collections::HashMap;

use async_openai::config::OpenAIConfig;
use async_openai::types::embeddings::{CreateEmbeddingRequestArgs, EmbeddingInput};
use async_openai::Client as OpenAiClient;
use tracing::info;

use crate::config::EmbeddingConfig;

pub struct EmbeddingService {
    config: EmbeddingConfig,
    client: OpenAiClient<OpenAIConfig>,
}

impl EmbeddingService {
    pub fn from_config(config: EmbeddingConfig) -> Self {
        let oai_config = OpenAIConfig::new().with_api_key(&config.api_key);
        let client = OpenAiClient::with_config(oai_config);
        Self { config, client }
    }

    /// Embed all messages via OpenAI API. Returns a map from message text
    /// to its embedding vector. Call this once at startup.
    pub async fn embed_all(
        &self,
        messages: &[String],
    ) -> Result<HashMap<String, Vec<f32>>, Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Embedding {} messages with model={} dims={}",
            messages.len(),
            self.config.model,
            self.config.dimensions,
        );

        let mut request = CreateEmbeddingRequestArgs::default();
        request
            .model(&self.config.model)
            .input(EmbeddingInput::StringArray(messages.to_vec()))
            .dimensions(self.config.dimensions);

        let request = request.build()?;
        let response = self.client.embeddings().create(request).await?;

        let mut map = HashMap::with_capacity(messages.len());
        for (i, embedding) in response.data.iter().enumerate() {
            map.insert(messages[i].clone(), embedding.embedding.to_vec());
        }

        info!("Embedded {} messages successfully", map.len());
        Ok(map)
    }
}
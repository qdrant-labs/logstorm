use async_trait::async_trait;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, CreateFieldIndexCollection, Distance, DocumentBuilder, FieldType,
    Modifier, NamedVectors, PointStruct, SparseVectorParamsBuilder, SparseVectorsConfigBuilder,
    UpsertPointsBuilder, VectorParamsBuilder, VectorsConfigBuilder,
};
use qdrant_client::{Payload, Qdrant};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::IndexMode;
use crate::log_entry::LogEntry;
use crate::sink::Sink;
use crate::sink::{DEFAULT_INDEX_NAME, DENSE_EMBEDDING_NAME, SPARSE_EMBEDDING_NAME};

fn default_collection_name() -> String {
    DEFAULT_INDEX_NAME.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantConfig {
    pub url: String,
    pub api_key: Option<String>,
    #[serde(default = "default_collection_name")]
    pub collection_name: String,
    #[serde(default)]
    pub index_mode: IndexMode,
}

pub struct QdrantSink {
    config: QdrantConfig,
    client: Qdrant,
}

impl QdrantSink {
    pub async fn from_config(config: QdrantConfig, embedding_dim: usize) -> Self {
        let mut qbuilder = Qdrant::from_url(&config.url);

        // grab api key from config if provided and set it on the builder
        if let Some(api_key) = &config.api_key {
            qbuilder = qbuilder.api_key(api_key.to_string());
        }

        let client = qbuilder.build().expect("Failed to create Qdrant client");

        // check if the collection exists by listing collections and looking for a match on the name
        let collection_exists = client
            .list_collections()
            .await
            .unwrap()
            .collections
            .iter()
            .any(|c| c.name == config.collection_name);

        // build collection if it doesn't exist
        // (creating a payload index on "level" and "service" for querying)
        if !collection_exists {
            let mut collection_builder =
                CreateCollectionBuilder::new(config.collection_name.clone());

            // dense vectors for semantic search
            if config.index_mode.needs_embeddings() {
                let mut vectors_config = VectorsConfigBuilder::default();
                vectors_config.add_named_vector_params(
                    DENSE_EMBEDDING_NAME,
                    VectorParamsBuilder::new(embedding_dim as u64, Distance::Cosine),
                );
                collection_builder = collection_builder.vectors_config(vectors_config);
            }

            // sparse vectors for BM25 keyword search
            if matches!(config.index_mode, IndexMode::Keyword | IndexMode::Hybrid) {
                let mut sparse_vectors_config = SparseVectorsConfigBuilder::default();
                sparse_vectors_config.add_named_vector_params(
                    SPARSE_EMBEDDING_NAME,
                    SparseVectorParamsBuilder::default().modifier(Modifier::Idf),
                );
                collection_builder =
                    collection_builder.sparse_vectors_config(sparse_vectors_config);
            }

            client
                .create_collection(collection_builder)
                .await
                .unwrap();

            // payload index on "level" field
            let payload_index = CreateFieldIndexCollection {
                collection_name: config.collection_name.clone(),
                field_name: "level".to_string(),
                field_type: Some(FieldType::Keyword.into()),
                field_index_params: None,
                wait: Some(true),
                ordering: None,
            };
            client.create_field_index(payload_index).await.unwrap();

            // payload index on "service" field
            let payload_index = CreateFieldIndexCollection {
                collection_name: config.collection_name.clone(),
                field_name: "service".to_string(),
                field_type: Some(FieldType::Keyword.into()),
                field_index_params: None,
                wait: Some(true),
                ordering: None,
            };
            client.create_field_index(payload_index).await.unwrap();
        }

        Self { config, client }
    }
}

#[async_trait]
impl Sink for QdrantSink {
    fn supported_modes(&self) -> &[IndexMode] {
        &[IndexMode::Vector, IndexMode::Keyword, IndexMode::Hybrid]
    }

    async fn write(
        &self,
        batch: &[LogEntry],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let index_mode = &self.config.index_mode;

        self.client
            .upsert_points(UpsertPointsBuilder::new(
                self.config.collection_name.clone(),
                batch
                    .iter()
                    .map(|entry| {
                        let mut vectors = NamedVectors::default();

                        if index_mode.needs_embeddings() {
                            vectors =
                                vectors.add_vector(DENSE_EMBEDDING_NAME, entry.embedding.clone());
                        }

                        if matches!(index_mode, IndexMode::Keyword | IndexMode::Hybrid) {
                            vectors = vectors.add_vector(
                                SPARSE_EMBEDDING_NAME,
                                DocumentBuilder::new(entry.message.clone(), "qdrant/bm25").build(),
                            );
                        }

                        PointStruct::new(
                            entry.id.clone(),
                            vectors,
                            Payload::try_from(json!({
                                "service": entry.service.clone(),
                                "level": format!("{:?}", entry.level),
                                "message": entry.message.clone(),
                                "timestamp": entry.timestamp,
                            }))
                            .unwrap(),
                        )
                    })
                    .collect::<Vec<PointStruct>>(),
            ))
            .await
            .unwrap();
        Ok(())
    }
}

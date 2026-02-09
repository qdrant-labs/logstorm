use async_trait::async_trait;
use qdrant_client::{Payload, Qdrant};
use qdrant_client::qdrant::{CreateCollectionBuilder, CreateFieldIndexCollection, Distance, DocumentBuilder, FieldType, Modifier, NamedVectors, PointStruct, SparseVectorParamsBuilder, SparseVectorsConfigBuilder, UpsertPointsBuilder, VectorParamsBuilder, VectorsConfigBuilder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::log_entry::LogEntry;
use crate::sink::Sink;

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

pub struct QdrantSink {
    config: QdrantConfig,
    client: Qdrant,
}

impl QdrantSink {
    pub async fn from_config(config: QdrantConfig) -> Self {
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

            let mut vectors_config = VectorsConfigBuilder::default();
            vectors_config.add_named_vector_params(
                "dense",
                VectorParamsBuilder::new(1536, Distance::Cosine)
            );

            let mut sparse_vectors_config = SparseVectorsConfigBuilder::default();
            sparse_vectors_config.add_named_vector_params(
                "bm25",
                // use the IDF modifier for BM25
                SparseVectorParamsBuilder::default().modifier(Modifier::Idf), 
            );

            client
                .create_collection(
            CreateCollectionBuilder::new(config.collection_name.clone())
                        // todo: make these vector params configurable???
                        .vectors_config(vectors_config)
                        .sparse_vectors_config(sparse_vectors_config)
                )
                .await
                .unwrap();
            
            // payload index on "level" field
            let payload_index = CreateFieldIndexCollection {
                collection_name: config.collection_name.clone(),
                field_name: "level".to_string(),
                field_type: Some(FieldType::Keyword.into()),
                field_index_params: None, // use optional parameters
                wait: Some(true), // wait for index creation to complete
                ordering: None, // default ordering
            };
            client.create_field_index(payload_index).await.unwrap();

            // payload index on "service" field
            let payload_index = CreateFieldIndexCollection {
                collection_name: config.collection_name.clone(),
                field_name: "service".to_string(),
                field_type: Some(FieldType::Keyword.into()),
                field_index_params: None, // use optional parameters
                wait: Some(true), // wait for index creation to complete
                ordering: None, // default ordering
            };
            client.create_field_index(payload_index).await.unwrap();
        }
        
        Self { config, client }
    }
}

#[async_trait]
impl Sink for QdrantSink {
    async fn write(
        &self,
        batch: &[LogEntry],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // upsert all of these logs into the collection
        self.client.upsert_points(
            UpsertPointsBuilder::new(
                // todo: do I have to clone this?
                self.config.collection_name.clone(),
                batch.iter().map(|entry| {
                    PointStruct::new(
                        Uuid::new_v4().to_string(),
                        NamedVectors::default()
                            .add_vector("dense", entry.embedding.clone())
                            .add_vector("bm25", DocumentBuilder::new(entry.message.clone(), "qdrant/bm25").build()),
                            Payload::try_from(json!({
                                "service": entry.service.clone(),
                                "level": format!("{:?}", entry.level),
                                "message": entry.message.clone(),
                                "timestamp": entry.timestamp,
                            })).unwrap()
                    )
                })
                .collect::<Vec<PointStruct>>()
            )
        )
        .await
        .unwrap();
        Ok(())
    }
}
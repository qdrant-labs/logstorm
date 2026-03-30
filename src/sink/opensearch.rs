use async_trait::async_trait;
use opensearch::{
    BulkOperation, BulkParts, OpenSearch,
    auth::Credentials,
    http::transport::{SingleNodeConnectionPool, TransportBuilder},
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::IndexMode;
use crate::log_entry::LogEntry;
use crate::sink::Sink;
use crate::sink::{DEFAULT_INDEX_NAME, DENSE_EMBEDDING_NAME};

fn default_index_name() -> String {
    DEFAULT_INDEX_NAME.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenSearchConfig {
    pub url: String,
    pub user: String,
    pub password: String,
    #[serde(default = "default_index_name")]
    pub index_name: String,
    #[serde(default)]
    pub index_mode: IndexMode,
    #[serde(default = "default_skip_cert")]
    pub skip_cert: bool,
}

// by default we want to validate certs, so we set this to false
fn default_skip_cert() -> bool {
    false
}

pub struct OpenSearchSink {
    config: OpenSearchConfig,
    client: OpenSearch,
}

impl OpenSearchSink {
    pub async fn from_config(config: OpenSearchConfig, embedding_dim: usize) -> Self {
        let credentials = Credentials::Basic(config.user.clone(), config.password.clone());
        let conn_pool = SingleNodeConnectionPool::new(config.url.clone().parse().unwrap());
        let transport = TransportBuilder::new(conn_pool)
            .auth(credentials)
            .cert_validation(if config.skip_cert {
                opensearch::cert::CertificateValidation::None
            } else {
                opensearch::cert::CertificateValidation::Default
            })
            .build()
            .expect("Failed to create OpenSearch transport");
        let client = OpenSearch::new(transport);

        let index_exists = client
            .indices()
            .exists(opensearch::indices::IndicesExistsParts::Index(&[
                &config.index_name,
            ]))
            .send()
            .await
            .expect("Failed to check if index exists")
            .status_code()
            == 200;

        if !index_exists {
            let mut properties = json!({
                "timestamp": { "type": "date" },
                "service": { "type": "keyword" },
                "level": { "type": "keyword" },
                "message": { "type": "text" },
            });

            if config.index_mode.needs_embeddings() {
                properties.as_object_mut().unwrap().insert(
                    DENSE_EMBEDDING_NAME.to_string(),
                    json!({
                        "type": "knn_vector",
                        "dimension": embedding_dim,
                        "method": {
                            "name": "hnsw",
                            "space_type": "cosinesimil",
                            "engine": "nmslib",
                        }
                    }),
                );
            }

            client
                .indices()
                .create(opensearch::indices::IndicesCreateParts::Index(
                    &config.index_name,
                ))
                .body(json!({
                    "settings": {
                        "index": { "knn": true }
                    },
                    "mappings": { "properties": properties }
                }))
                .send()
                .await
                .expect("Failed to create index");
        }

        Self { config, client }
    }
}

#[async_trait]
impl Sink for OpenSearchSink {
    fn supported_modes(&self) -> &[IndexMode] {
        &[IndexMode::Vector, IndexMode::Keyword, IndexMode::Hybrid]
    }

    async fn write(
        &self,
        batch: &[LogEntry],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let include_embedding = self.config.index_mode.needs_embeddings();

        let logs = batch
            .iter()
            .map(|entry| {
                let id = entry.id.clone();
                let mut doc = json!({
                    "timestamp": entry.timestamp,
                    "service": entry.service,
                    "level": format!("{:?}", entry.level),
                    "message": entry.message,
                });

                if include_embedding {
                    doc.as_object_mut()
                        .unwrap()
                        .insert(DENSE_EMBEDDING_NAME.to_string(), json!(entry.embedding));
                }

                BulkOperation::index(doc).id(&id).routing(&id).into()
            })
            .collect::<Vec<BulkOperation<_>>>();

        self.client
            .bulk(BulkParts::Index(&self.config.index_name))
            .body(logs)
            .send()
            .await
            .expect("Failed to write logs to OpenSearch");

        Ok(())
    }
}
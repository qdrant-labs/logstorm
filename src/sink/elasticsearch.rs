use async_trait::async_trait;
use elasticsearch::{
    BulkOperation, BulkParts, Elasticsearch as EsClient, auth::Credentials, http::transport::{SingleNodeConnectionPool, TransportBuilder}
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::log_entry::LogEntry;
use crate::sink::Sink;
use crate::sink::{
    DEFAULT_INDEX_NAME,
    DENSE_EMBEDDING_NAME
};

fn default_index_name() -> String {
    DEFAULT_INDEX_NAME.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElasticSearchConfig {
    pub url: String,
    pub user: String,
    pub password: String,
    #[serde(default = "default_index_name")]
    pub index_name: String,
}

pub struct ElasticSearchSink {
    config: ElasticSearchConfig,
    client: EsClient,
}

impl ElasticSearchSink {
    pub async fn from_config(config: ElasticSearchConfig, embedding_dim: usize) -> Self {

        // build the Elasticsearch client
        let credentials = Credentials::Basic(config.user.clone(), config.password.clone());
        let conn_pool = SingleNodeConnectionPool::new(config.url.clone().parse().unwrap());
        let transport = TransportBuilder::new(conn_pool)
            .auth(credentials)
            .build()
            .expect("Failed to create Elasticsearch transport");
        let client = EsClient::new(transport);
        
        // create the index if it doesn't exist
        let index_exists = client
            .indices()
            .exists(elasticsearch::indices::IndicesExistsParts::Index(&[&config.index_name]))
            .send()
            .await
            .expect("Failed to check if index exists")
            .status_code()
            == 200; 

        if !index_exists {
            client
                .indices()
                .create(elasticsearch::indices::IndicesCreateParts::Index(&config.index_name))
                .body(json!({
                    "mappings": {
                        "properties": {
                            "timestamp": { "type": "date" },
                            "service": { "type": "keyword" },
                            "level": { "type": "keyword" },
                            "message": { "type": "text" },
                            DENSE_EMBEDDING_NAME: {
                                "type": "dense_vector",
                                "dims": embedding_dim,
                                "index": true,
                                "index_options": {
                                    "type": "hnsw",
                                }
                            }
                        }
                    }
                }))
                .send()
                .await
                .expect("Failed to create index");
        }

        Self { config, client }
    }
}

#[async_trait]
impl Sink for ElasticSearchSink {
    async fn write(
        &self,
        batch: &[LogEntry],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let logs = batch
            .iter()
            .map(|entry| {
                let id = entry.id.clone();
                BulkOperation::index(json!({
                    "timestamp": entry.timestamp,
                    "service": entry.service,
                    "level": format!("{:?}", entry.level),
                    "message": entry.message,
                    DENSE_EMBEDDING_NAME: entry.embedding,
                })).id(&id).routing(&id).into()
            })
            .collect::<Vec<BulkOperation<_>>>();

        self.client
            .bulk(BulkParts::Index(&self.config.index_name))
            .body(logs)
            .send()
            .await
            .expect("Failed to write logs to Elasticsearch");

        Ok(())
    }
}
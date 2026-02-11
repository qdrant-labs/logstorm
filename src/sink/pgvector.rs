use async_trait::async_trait;
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::log_entry::LogEntry;
use crate::sink::Sink;
use crate::sink::DEFAULT_INDEX_NAME;

fn default_table_name() -> String {
    DEFAULT_INDEX_NAME.to_string()
}

fn default_port() -> u16 {
    5432
}

fn default_user() -> String {
    "postgres".to_string()
}

fn default_password() -> String {
    "changeme".to_string()
}

fn default_database() -> String {
    "postgres".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PgvectorConfig {
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_user")]
    pub user: String,
    #[serde(default = "default_password")]
    pub password: String,
    #[serde(default = "default_database")]
    pub database: String,
    #[serde(default = "default_table_name")]
    pub table_name: String,
}

pub struct PgvectorSink {
    config: PgvectorConfig,
    pool: PgPool,
}

impl PgvectorSink {
    pub async fn from_config(config: PgvectorConfig, embedding_dim: usize) -> Self {
        let url = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.user, config.password, config.host, config.port, config.database,
        );

        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(&url)
            .await
            .expect("Failed to connect to Postgres");

        // ensure pgvector extension is available
        sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
            .execute(&pool)
            .await
            .expect("Failed to create vector extension");

        // create table if it doesn't exist
        let create_table = format!(
            r#"CREATE TABLE IF NOT EXISTS {} (
                id TEXT PRIMARY KEY,
                timestamp TIMESTAMPTZ NOT NULL,
                service TEXT NOT NULL,
                level TEXT NOT NULL,
                message TEXT NOT NULL,
                message_tsv TSVECTOR GENERATED ALWAYS AS (to_tsvector('english', message)) STORED,
                embedding vector({})
            )"#,
            config.table_name, embedding_dim,
        );
        sqlx::query(&create_table)
            .execute(&pool)
            .await
            .expect("Failed to create table");

        // create an HNSW index on the embedding column for cosine similarity
        let create_index = format!(
            r#"CREATE INDEX IF NOT EXISTS {table}_embedding_idx
               ON {table} USING hnsw (embedding vector_cosine_ops)"#,
            table = config.table_name,
        );
        sqlx::query(&create_index)
            .execute(&pool)
            .await
            .expect("Failed to create HNSW index");

        // create a GIN index on the message column for full-text search
        let create_fts_index = format!(
            r#"CREATE INDEX IF NOT EXISTS {table}_message_idx
               ON {table} USING GIN (message_tsv)"#,
            table = config.table_name,
        );
        sqlx::query(&create_fts_index)
            .execute(&pool)
            .await
            .expect("Failed to create GIN index");

        Self { config, pool }
    }
}

#[async_trait]
impl Sink for PgvectorSink {
    async fn write(
        &self,
        batch: &[LogEntry],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // build a batch insert using UNNEST for efficiency
        let mut ids = Vec::with_capacity(batch.len());
        let mut timestamps = Vec::with_capacity(batch.len());
        let mut services = Vec::with_capacity(batch.len());
        let mut levels = Vec::with_capacity(batch.len());
        let mut messages = Vec::with_capacity(batch.len());
        let mut embeddings: Vec<Vector> = Vec::with_capacity(batch.len());

        for entry in batch {
            ids.push(entry.id.clone());
            timestamps.push(entry.timestamp);
            services.push(entry.service.clone());
            levels.push(format!("{:?}", entry.level));
            messages.push(entry.message.clone());
            embeddings.push(Vector::from(entry.embedding.clone()));
        }

        let query = format!(
            r#"INSERT INTO {} (id, timestamp, service, level, message, embedding)
               SELECT * FROM UNNEST($1::text[], $2::timestamptz[], $3::text[], $4::text[], $5::text[], $6::vector[])
               ON CONFLICT (id) DO NOTHING"#,
            self.config.table_name,
        );

        sqlx::query(&query)
            .bind(&ids)
            .bind(&timestamps)
            .bind(&services)
            .bind(&levels)
            .bind(&messages)
            .bind(&embeddings)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
use async_trait::async_trait;
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::config::IndexMode;
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
    #[serde(default)]
    pub index_mode: IndexMode,
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

        let include_vectors = config.index_mode.needs_embeddings();
        let include_keyword = matches!(config.index_mode, IndexMode::Keyword | IndexMode::Hybrid);

        if include_vectors {
            sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
                .execute(&pool)
                .await
                .expect("Failed to create vector extension");
        }

        // build CREATE TABLE dynamically based on index_mode
        let mut columns = vec![
            "id TEXT PRIMARY KEY".to_string(),
            "timestamp TIMESTAMPTZ NOT NULL".to_string(),
            "service TEXT NOT NULL".to_string(),
            "level TEXT NOT NULL".to_string(),
            "message TEXT NOT NULL".to_string(),
        ];

        if include_keyword {
            columns.push(
                "message_tsv TSVECTOR GENERATED ALWAYS AS (to_tsvector('english', message)) STORED"
                    .to_string(),
            );
        }

        if include_vectors {
            columns.push(format!("embedding vector({})", embedding_dim));
        }

        let create_table = format!(
            "CREATE TABLE IF NOT EXISTS {} ({})",
            config.table_name,
            columns.join(", "),
        );
        sqlx::query(&create_table)
            .execute(&pool)
            .await
            .expect("Failed to create table");

        if include_vectors {
            let create_index = format!(
                r#"CREATE INDEX IF NOT EXISTS {table}_embedding_idx
                   ON {table} USING hnsw (embedding vector_cosine_ops)"#,
                table = config.table_name,
            );
            sqlx::query(&create_index)
                .execute(&pool)
                .await
                .expect("Failed to create HNSW index");
        }

        if include_keyword {
            let create_fts_index = format!(
                r#"CREATE INDEX IF NOT EXISTS {table}_message_idx
                   ON {table} USING GIN (message_tsv)"#,
                table = config.table_name,
            );
            sqlx::query(&create_fts_index)
                .execute(&pool)
                .await
                .expect("Failed to create GIN index");
        }

        Self { config, pool }
    }
}

#[async_trait]
impl Sink for PgvectorSink {
    fn supported_modes(&self) -> &[IndexMode] {
        &[IndexMode::Vector, IndexMode::Keyword, IndexMode::Hybrid]
    }

    async fn write(
        &self,
        batch: &[LogEntry],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let include_vectors = self.config.index_mode.needs_embeddings();

        let mut ids = Vec::with_capacity(batch.len());
        let mut timestamps = Vec::with_capacity(batch.len());
        let mut services = Vec::with_capacity(batch.len());
        let mut levels = Vec::with_capacity(batch.len());
        let mut messages = Vec::with_capacity(batch.len());

        for entry in batch {
            ids.push(entry.id.clone());
            timestamps.push(entry.timestamp);
            services.push(entry.service.clone());
            levels.push(format!("{:?}", entry.level));
            messages.push(entry.message.clone());
        }

        if include_vectors {
            let mut embeddings: Vec<Vector> = Vec::with_capacity(batch.len());
            for entry in batch {
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
        } else {
            let query = format!(
                r#"INSERT INTO {} (id, timestamp, service, level, message)
                   SELECT * FROM UNNEST($1::text[], $2::timestamptz[], $3::text[], $4::text[], $5::text[])
                   ON CONFLICT (id) DO NOTHING"#,
                self.config.table_name,
            );

            sqlx::query(&query)
                .bind(&ids)
                .bind(&timestamps)
                .bind(&services)
                .bind(&levels)
                .bind(&messages)
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }
}
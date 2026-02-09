use async_trait::async_trait;
use crate::log_entry::LogEntry;

#[async_trait]
pub trait Sink: Send + Sync {
    async fn write(&self, batch: &[LogEntry]) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Prints a summary per flush. Swap this out for Elastic/Qdrant sinks later.
pub struct StdoutSink;

#[async_trait]
impl Sink for StdoutSink {
    async fn write(&self, batch: &[LogEntry]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("[flush] {} logs", batch.len());
        Ok(())
    }
}
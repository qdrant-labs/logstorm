use async_trait::async_trait;

use tracing::{debug, error, info, warn};

use crate::log_entry::LogEntry;

#[async_trait]
pub trait Sink: Send + Sync {
    async fn write(
        &self,
        batch: &[LogEntry],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// Prints a summary per flush. Swap this out for Elastic/Qdrant sinks later.
pub struct StdoutSink;

#[async_trait]
impl Sink for StdoutSink {
    async fn write(
        &self,
        batch: &[LogEntry],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        for entry in batch {
            match entry.level {
                crate::log_entry::LogLevel::Debug => debug!("{}: {}", entry.service, entry.message),
                crate::log_entry::LogLevel::Info => info!("{}: {}", entry.service, entry.message),
                crate::log_entry::LogLevel::Warn => warn!("{}: {}", entry.service, entry.message),
                crate::log_entry::LogLevel::Error => error!("{}: {}", entry.service, entry.message),
            }
        }
        Ok(())
    }
}

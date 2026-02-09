use crate::log_entry::LogEntry;
use crate::sink::Sink;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub struct Buffer {
    rx: mpsc::Receiver<LogEntry>,
    sinks: Vec<Box<dyn Sink>>,
    capacity: usize,
    flush_interval: Duration,
}

impl Buffer {
    pub fn new(
        rx: mpsc::Receiver<LogEntry>,
        sinks: Vec<Box<dyn Sink>>,
        capacity: usize,
        flush_interval: Duration,
    ) -> Self {
        Self {
            rx,
            sinks,
            capacity,
            flush_interval,
        }
    }

    pub async fn run(&mut self) {
        let mut entries = Vec::with_capacity(self.capacity);
        let mut last_flush = Instant::now();

        loop {
            let timeout = self.flush_interval.saturating_sub(last_flush.elapsed());

            match tokio::time::timeout(timeout, self.rx.recv()).await {
                Ok(Some(entry)) => {
                    entries.push(entry);
                    if entries.len() >= self.capacity {
                        self.flush(&mut entries).await;
                        last_flush = Instant::now();
                    }
                }
                Ok(None) => {
                    // Channel closed — all emitters done
                    if !entries.is_empty() {
                        self.flush(&mut entries).await;
                    }
                    break;
                }
                Err(_) => {
                    // Timer expired — flush whatever we have
                    if !entries.is_empty() {
                        self.flush(&mut entries).await;
                        last_flush = Instant::now();
                    }
                }
            }
        }
    }

    async fn flush(&self, entries: &mut Vec<LogEntry>) {
        let batch = std::mem::replace(entries, Vec::with_capacity(self.capacity));
        for sink in &self.sinks {
            if let Err(e) = sink.write(&batch).await {
                eprintln!("Sink error: {e}");
            }
        }
    }
}

use std::collections::HashMap;
use std::time::Instant;

use async_trait::async_trait;
use axum::{
    Router,
    extract::State,
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::{Html, IntoResponse},
    routing::get,
};
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::info;

use crate::log_entry::LogEntry;
use crate::sink::Sink;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub port: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlushEvent {
    pub timestamp: DateTime<Utc>,
    pub total_count: usize,
    pub by_service: HashMap<String, usize>,
    pub by_level: HashMap<String, usize>,
    pub flush_duration_ms: u64,
}

pub struct DashboardSink {
    tx: broadcast::Sender<FlushEvent>,
}

impl DashboardSink {
    pub fn new(tx: broadcast::Sender<FlushEvent>) -> Self {
        Self { tx }
    }
}

#[async_trait]
impl Sink for DashboardSink {
    async fn write(
        &self,
        batch: &[LogEntry],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let start = Instant::now();

        let mut by_service: HashMap<String, usize> = HashMap::new();
        let mut by_level: HashMap<String, usize> = HashMap::new();

        for entry in batch {
            *by_service.entry(entry.service.clone()).or_default() += 1;
            *by_level
                .entry(format!("{}", entry.level))
                .or_default() += 1;
        }

        let event = FlushEvent {
            timestamp: Utc::now(),
            total_count: batch.len(),
            by_service,
            by_level,
            flush_duration_ms: start.elapsed().as_millis() as u64,
        };

        // Ignore send errors — just means no clients are connected
        let _ = self.tx.send(event);
        Ok(())
    }
}

pub async fn start_dashboard_server(port: u16, tx: broadcast::Sender<FlushEvent>) {
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .with_state(tx);

    let addr = format!("0.0.0.0:{port}");
    info!("Dashboard server listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind dashboard server");

    axum::serve(listener, app).await.expect("Dashboard server error");
}

async fn index_handler() -> impl IntoResponse {
    Html(DASHBOARD_HTML)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(tx): State<broadcast::Sender<FlushEvent>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, tx))
}

async fn handle_ws(socket: WebSocket, tx: broadcast::Sender<FlushEvent>) {
    let mut rx = tx.subscribe();
    let (mut sender, mut _receiver) = socket.split();

    while let Ok(event) = rx.recv().await {
        let json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(_) => continue,
        };
        if sender.send(Message::Text(json.into())).await.is_err() {
            break; // client disconnected
        }
    }
}

const DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Emitter Dashboard</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: system-ui, -apple-system, sans-serif; background: #0f172a; color: #e2e8f0; padding: 24px; }
  h1 { font-size: 1.5rem; margin-bottom: 16px; color: #38bdf8; }
  .status { margin-bottom: 16px; font-size: 0.875rem; color: #94a3b8; }
  .status .dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin-right: 6px; }
  .dot.connected { background: #22c55e; }
  .dot.disconnected { background: #ef4444; }
  .summary { display: flex; gap: 16px; margin-bottom: 24px; flex-wrap: wrap; }
  .card { background: #1e293b; border-radius: 8px; padding: 16px 20px; min-width: 140px; }
  .card .label { font-size: 0.75rem; color: #94a3b8; text-transform: uppercase; letter-spacing: 0.05em; }
  .card .value { font-size: 1.75rem; font-weight: 700; margin-top: 4px; color: #f1f5f9; }
  table { width: 100%; border-collapse: collapse; background: #1e293b; border-radius: 8px; overflow: hidden; }
  th { text-align: left; padding: 10px 14px; background: #334155; font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.05em; color: #94a3b8; }
  td { padding: 10px 14px; border-top: 1px solid #334155; font-size: 0.875rem; font-variant-numeric: tabular-nums; }
  tr:hover td { background: #1e3a5f; }
  .level-badge { padding: 2px 8px; border-radius: 4px; font-size: 0.75rem; font-weight: 600; }
  .level-DEBUG { background: #164e63; color: #67e8f9; }
  .level-INFO { background: #14532d; color: #86efac; }
  .level-WARN { background: #713f12; color: #fde047; }
  .level-ERROR { background: #7f1d1d; color: #fca5a5; }
</style>
</head>
<body>
<h1>Emitter Dashboard</h1>
<div class="status" id="status"><span class="dot disconnected" id="dot"></span>Connecting...</div>

<div class="summary">
  <div class="card"><div class="label">Total Flushes</div><div class="value" id="totalFlushes">0</div></div>
  <div class="card"><div class="label">Total Logs</div><div class="value" id="totalLogs">0</div></div>
  <div class="card"><div class="label">Last Batch</div><div class="value" id="lastBatch">-</div></div>
</div>

<table>
  <thead>
    <tr><th>Time</th><th>Count</th><th>Services</th><th>Levels</th><th>Duration</th></tr>
  </thead>
  <tbody id="events"></tbody>
</table>

<script>
  const MAX_ROWS = 50;
  let totalFlushes = 0;
  let totalLogs = 0;

  function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws`);
    const dot = document.getElementById('dot');
    const status = document.getElementById('status');

    ws.onopen = () => {
      dot.className = 'dot connected';
      status.innerHTML = '<span class="dot connected" id="dot"></span>Connected';
    };

    ws.onclose = () => {
      dot.className = 'dot disconnected';
      status.innerHTML = '<span class="dot disconnected" id="dot"></span>Reconnecting...';
      setTimeout(connect, 2000);
    };

    ws.onmessage = (msg) => {
      const ev = JSON.parse(msg.data);
      totalFlushes++;
      totalLogs += ev.total_count;

      document.getElementById('totalFlushes').textContent = totalFlushes;
      document.getElementById('totalLogs').textContent = totalLogs.toLocaleString();
      document.getElementById('lastBatch').textContent = ev.total_count;

      const tbody = document.getElementById('events');
      const tr = document.createElement('tr');

      const services = Object.entries(ev.by_service).map(([k,v]) => `${k}: ${v}`).join(', ');
      const levels = Object.entries(ev.by_level)
        .map(([k,v]) => `<span class="level-badge level-${k}">${k}: ${v}</span>`)
        .join(' ');
      const time = new Date(ev.timestamp).toLocaleTimeString();

      tr.innerHTML = `<td>${time}</td><td>${ev.total_count}</td><td>${services}</td><td>${levels}</td><td>${ev.flush_duration_ms}ms</td>`;
      tbody.prepend(tr);

      // keep table bounded
      while (tbody.children.length > MAX_ROWS) tbody.removeChild(tbody.lastChild);
    };
  }

  connect();
</script>
</body>
</html>
"##;

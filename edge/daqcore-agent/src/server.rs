// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Local control plane: Axum REST + WebSocket endpoints plus the embedded
//! dashboard, all speaking the same `/api/v1` contract.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use daqcore_core::{SampleBatch, Value};
use daqcore_driver::DriverMetadata;
use daqcore_wal::Wal;
use futures_util::{SinkExt, StreamExt};
use rust_embed::Embed;
use serde::Serialize;
use tokio::sync::{broadcast, Mutex};

/// The built dashboard bundle, embedded into the binary at compile time.
#[derive(Embed)]
#[folder = "../dashboard/dist/"]
struct Assets;

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    pub name: String,
    pub started: Instant,
    pub drivers: Vec<DriverMetadata>,
    pub wal: Arc<Mutex<Wal>>,
    pub live_tx: broadcast::Sender<Arc<SampleBatch>>,
    pub total_samples: Arc<AtomicU64>,
}

/// Build the router: API routes plus a fallback serving the embedded SPA.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/drivers", get(drivers))
        .route("/api/v1/metrics", get(metrics))
        .route("/api/v1/live", get(live))
        .fallback(static_handler)
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    name: String,
    uptime_secs: u64,
    written_seq: u64,
    acknowledged_seq: u64,
}

async fn health(State(s): State<AppState>) -> Json<Health> {
    let wal = s.wal.lock().await;
    Json(Health {
        status: "ok",
        name: s.name.clone(),
        uptime_secs: s.started.elapsed().as_secs(),
        written_seq: wal.written_seq(),
        acknowledged_seq: wal.acknowledged_seq(),
    })
}

#[derive(Serialize)]
struct Drivers {
    drivers: Vec<DriverMetadata>,
}

async fn drivers(State(s): State<AppState>) -> Json<Drivers> {
    Json(Drivers {
        drivers: s.drivers.clone(),
    })
}

#[derive(Serialize)]
struct Metrics {
    written_seq: u64,
    acknowledged_seq: u64,
    buffered: u64,
    total_samples: u64,
}

async fn metrics(State(s): State<AppState>) -> Json<Metrics> {
    let wal = s.wal.lock().await;
    let written = wal.written_seq();
    let acknowledged = wal.acknowledged_seq();
    Json(Metrics {
        written_seq: written,
        acknowledged_seq: acknowledged,
        buffered: written.saturating_sub(acknowledged),
        total_samples: s.total_samples.load(Ordering::Relaxed),
    })
}

/// Upgrade `/api/v1/live` to a WebSocket and stream batches as they arrive.
async fn live(ws: WebSocketUpgrade, State(s): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| live_stream(socket, s))
}

/// JS-safe sample: timestamps as milliseconds (f64), numeric values only.
#[derive(Serialize)]
struct LiveSample {
    ts_ms: f64,
    channel: String,
    value: f64,
}

#[derive(Serialize)]
struct LiveBatch {
    seq: u64,
    samples: Vec<LiveSample>,
}

fn numeric(value: &Value) -> Option<f64> {
    match value {
        Value::F64(v) => Some(*v),
        Value::I64(v) => Some(*v as f64),
        _ => None,
    }
}

async fn live_stream(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.live_tx.subscribe();

    loop {
        tokio::select! {
            // Drain client frames so pings/close are noticed.
            msg = receiver.next() => {
                if msg.is_none() {
                    break;
                }
            }
            // Forward the next batch to the client as a compact JSON text frame.
            batch = rx.recv() => {
                let Ok(batch) = batch else { break };
                let samples: Vec<LiveSample> = batch
                    .samples
                    .iter()
                    .filter_map(|s| {
                        numeric(&s.value).map(|value| LiveSample {
                            ts_ms: s.ts as f64 / 1e6,
                            channel: s.channel.clone(),
                            value,
                        })
                    })
                    .collect();
                let frame = LiveBatch { seq: batch.seq, samples };
                let Ok(json) = serde_json::to_string(&frame) else { continue };
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Serve the embedded dashboard, falling back to `index.html` for SPA routes.
async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return Response::builder()
            .header("content-type", mime.as_ref())
            .body(Body::from(content.data.into_owned()))
            .unwrap();
    }

    // No extension -> likely a client-side route, serve the app shell.
    if !path.contains('.') {
        if let Some(content) = Assets::get("index.html") {
            return Response::builder()
                .header("content-type", "text/html")
                .body(Body::from(content.data.into_owned()))
                .unwrap();
        }
    }

    (StatusCode::NOT_FOUND, "not found").into_response()
}

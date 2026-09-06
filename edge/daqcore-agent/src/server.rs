// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Local control plane: Axum REST + WebSocket endpoints for the embedded
//! dashboard, the CLI, and (later) the cloud — all speaking the same `/api/v1`
//! contract.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use daqcore_core::SampleBatch;
use daqcore_driver::DriverMetadata;
use daqcore_wal::Wal;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::{broadcast, Mutex};

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

/// Build the router with all `/api/v1` routes.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/v1/health", get(health))
        .route("/api/v1/drivers", get(drivers))
        .route("/api/v1/metrics", get(metrics))
        .route("/api/v1/live", get(live))
        .with_state(state)
}

async fn index() -> &'static str {
    "DAQcore edge agent — /api/v1/health /api/v1/drivers /api/v1/metrics (WS: /api/v1/live)"
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
            // Forward the next batch to the client as a JSON text frame.
            batch = rx.recv() => {
                let Ok(batch) = batch else { break };
                let Ok(json) = serde_json::to_string(batch.as_ref()) else { continue };
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

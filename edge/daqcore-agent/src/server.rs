// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Local control plane: Axum REST + WebSocket endpoints plus the embedded
//! dashboard, all speaking the same `/api/v1` contract.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use daqcore_core::{Command, Sample, SampleBatch, Value};
use daqcore_driver::{
    ChannelInfo, Driver, DriverMetadata, MockBenchConfig, MockChannelConfig, SyntheticMockBench,
};
use daqcore_wal::Wal;
use futures_util::{SinkExt, StreamExt};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Mutex};

use crate::devices::{DeviceManager, SharedDriver};

/// The built dashboard bundle, embedded into the binary at compile time.
#[derive(Embed)]
#[folder = "../dashboard/dist/"]
struct Assets;

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    pub name: String,
    pub started: Instant,
    pub devices: Arc<std::sync::Mutex<DeviceManager>>,
    pub wal: Arc<Mutex<Wal>>,
    pub live_tx: broadcast::Sender<Arc<SampleBatch>>,
    pub total_samples: Arc<AtomicU64>,
    pub sys: Arc<Mutex<sysinfo::System>>,
}

type ApiResult<T> = Result<T, (StatusCode, String)>;

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn not_found(msg: &str) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, msg.to_string())
}

/// Build the router: API routes plus a fallback serving the embedded SPA.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/v1/health", get(health))
        .route("/api/v1/drivers", get(drivers))
        .route("/api/v1/metrics", get(metrics))
        .route("/api/v1/system", get(system))
        .route("/api/v1/devices", get(devices).post(add_device))
        .route("/api/v1/devices/{id}/sample", post(device_sample))
        .route("/api/v1/devices/{id}/command", post(device_command))
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
    let mgr = s.devices.lock().unwrap();
    Json(Drivers {
        drivers: mgr.metadata(),
    })
}

#[derive(Serialize)]
struct DeviceInfo {
    id: String,
    kind: String,
    connected: bool,
    channels: Vec<ChannelInfo>,
}

#[derive(Serialize)]
struct Devices {
    devices: Vec<DeviceInfo>,
}

async fn devices(State(s): State<AppState>) -> Json<Devices> {
    let mgr = s.devices.lock().unwrap();
    let devices = mgr
        .shared_drivers()
        .into_iter()
        .filter_map(|(id, _)| {
            mgr.get(&id).map(|e| DeviceInfo {
                id,
                kind: e.metadata.kind.clone(),
                connected: e.connected,
                channels: e.metadata.channels.clone(),
            })
        })
        .collect();
    Json(Devices { devices })
}

/// Add a device at runtime. Currently supports the `mock` driver type.
#[derive(Deserialize)]
struct AddDeviceRequest {
    id: String,
    #[serde(rename = "type", default = "default_kind")]
    kind: String,
    #[serde(default)]
    channels: Vec<MockChannelConfig>,
}

fn default_kind() -> String {
    "mock".to_string()
}

async fn add_device(
    State(s): State<AppState>,
    Json(req): Json<AddDeviceRequest>,
) -> ApiResult<Json<DeviceInfo>> {
    if req.kind != "mock" {
        return Err(not_found(&format!(
            "unsupported device type `{}`",
            req.kind
        )));
    }

    let cfg = MockBenchConfig {
        id: req.id.clone(),
        channels: req.channels,
    };
    let mut driver = SyntheticMockBench::new(cfg);
    driver.connect().await.map_err(internal)?;
    let metadata = driver.metadata();

    let shared: SharedDriver = Arc::new(Mutex::new(Box::new(driver)));
    {
        let mut mgr = s.devices.lock().unwrap();
        mgr.insert(req.id.clone(), metadata.clone(), shared);
    }

    info_device(&req.id, &metadata);
    Ok(Json(DeviceInfo {
        id: req.id,
        kind: metadata.kind,
        connected: true,
        channels: metadata.channels,
    }))
}

fn info_device(id: &str, metadata: &DriverMetadata) {
    tracing::info!(device = %id, kind = %metadata.kind, channels = ?metadata.channels, "device added");
}

#[derive(Serialize)]
struct SampleResponse {
    device: String,
    samples: Vec<LiveSample>,
}

async fn device_sample(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<SampleResponse>> {
    let driver = {
        let mgr = s.devices.lock().unwrap();
        mgr.get(&id)
            .map(|e| e.driver.clone())
            .ok_or_else(|| not_found("device not found"))?
    };
    let mut d = driver.lock().await;
    let samples = d.sample().await.map_err(internal)?;
    Ok(Json(SampleResponse {
        device: id,
        samples: to_live_samples(&samples),
    }))
}

#[derive(Deserialize)]
struct CommandRequest {
    op: String,
    #[serde(default)]
    args: Vec<Value>,
}

async fn device_command(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(req): Json<CommandRequest>,
) -> ApiResult<Json<daqcore_core::CommandResponse>> {
    let driver = {
        let mgr = s.devices.lock().unwrap();
        mgr.get(&id)
            .map(|e| e.driver.clone())
            .ok_or_else(|| not_found("device not found"))?
    };
    let mut d = driver.lock().await;
    let cmd = Command {
        device: id,
        op: req.op,
        args: req.args,
    };
    let resp = d.send_command(cmd).await.map_err(internal)?;
    Ok(Json(resp))
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

#[derive(Serialize)]
struct SystemInfo {
    cpu_percent: f32,
    mem_used_bytes: u64,
    mem_total_bytes: u64,
}

async fn system(State(s): State<AppState>) -> Json<SystemInfo> {
    let mut sys = s.sys.lock().await;
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    Json(SystemInfo {
        cpu_percent: sys.global_cpu_usage(),
        mem_used_bytes: sys.used_memory(),
        mem_total_bytes: sys.total_memory(),
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

fn to_live_samples(samples: &[Sample]) -> Vec<LiveSample> {
    samples
        .iter()
        .filter_map(|s| {
            numeric(&s.value).map(|value| LiveSample {
                ts_ms: s.ts as f64 / 1e6,
                channel: s.channel.clone(),
                value,
            })
        })
        .collect()
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
                let frame = LiveBatch {
                    seq: batch.seq,
                    samples: to_live_samples(&batch.samples),
                };
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

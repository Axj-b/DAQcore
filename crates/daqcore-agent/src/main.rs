// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! DAQcore edge agent: sample configured drivers into a crash-safe local WAL.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use daqcore_core::{Error, Result, Sample, SampleBatch};
use daqcore_driver::{Driver, DriverRegistry};
use daqcore_wal::{Wal, WalConfig};
use tokio::sync::Mutex;
use tracing::{debug, error, info};
use tracing_subscriber::EnvFilter;

mod config;
use config::Config;

#[derive(Parser, Debug)]
#[command(
    name = "daqcore-agent",
    version,
    about = "DAQcore edge agent: stream drivers into a crash-safe local WAL"
)]
struct Args {
    /// Path to the agent config file (TOML).
    #[arg(short, long, default_value = "config/default.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        error!("agent fatal: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    // Logging (level via RUST_LOG env, defaults to info).
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    // Parse CLI args and load the config twice: once typed (agent/storage/sampling),
    // once as raw TOML so any driver type can read its own schema from it.
    let args = Args::parse();
    let text = fs::read_to_string(&args.config)
        .map_err(|e| Error::Config(format!("read {}: {e}", args.config.display())))?;
    let config: Config =
        toml::from_str(&text).map_err(|e| Error::Config(format!("parse config: {e}")))?;
    let raw: toml::Value =
        toml::from_str(&text).map_err(|e| Error::Config(format!("parse config: {e}")))?;

    // Build a driver per `[[device]]` block, keyed by its `type`.
    let registry = DriverRegistry::new().with_mock();
    let mut drivers: Vec<Box<dyn Driver>> = Vec::new();

    if let Some(devices) = raw.get("device").and_then(|v| v.as_array()) {
        for device in devices {
            let table = device
                .as_table()
                .ok_or_else(|| Error::Config("device entry must be a table".into()))?;
            let kind = table
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::Config("device entry missing `type`".into()))?;
            drivers.push(registry.build(kind, table)?);
        }
    }

    if drivers.is_empty() {
        return Err(Error::Config("no devices configured".into()));
    }

    // Connect every driver before starting the sampling loop.
    for driver in &mut drivers {
        driver.connect().await?;
        let m = driver.metadata();
        info!(
            device = %m.id,
            kind = %m.kind,
            channels = ?m.channels,
            "driver connected"
        );
    }

    // Open the WAL (shared behind a mutex so we can also ack from here).
    let wal = Arc::new(Mutex::new(Wal::open(WalConfig {
        dir: config.storage.wal_dir.clone(),
        segment_size: config.storage.segment_size_bytes,
        durability: config.storage.durability()?,
    })?));

    info!(
        name = %config.agent.name,
        wal_dir = %config.storage.wal_dir.display(),
        interval_ms = config.sampling.interval_ms,
        batch_size = config.sampling.batch_size,
        "agent started"
    );

    // Sampling loop state: continue from the WAL's recovered cursor.
    let batch_size = config.sampling.batch_size.max(1);
    let mut seq = wal.lock().await.written_seq();
    let mut accum: Vec<Sample> = Vec::new();
    let mut total_samples = 0u64;
    let mut tick = tokio::time::interval(Duration::from_millis(config.sampling.interval_ms.max(1)));

    loop {
        tokio::select! {
            // Ctrl+C / SIGTERM -> flush and stop.
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown requested, flushing");
                break;
            }
            // Every interval: read all drivers, then drain full batches to the WAL.
            _ = tick.tick() => {
                for driver in &mut drivers {
                    match driver.sample().await {
                        Ok(mut samples) => accum.append(&mut samples),
                        Err(e) => error!(error = %e, "sample failed"),
                    }
                }

                while accum.len() >= batch_size {
                    let samples: Vec<Sample> = accum.drain(..batch_size).collect();
                    seq += 1;
                    let batch = SampleBatch { seq, samples };
                    let mut w = wal.lock().await;
                    w.append(&batch)?;
                    w.acknowledge(seq);
                    total_samples += batch.samples.len() as u64;
                    debug!(seq, samples = batch.samples.len(), "batch written");
                }
            }
        }
    }

    // Write out any leftover samples on shutdown.
    if !accum.is_empty() {
        seq += 1;
        let batch = SampleBatch {
            seq,
            samples: accum,
        };
        let mut w = wal.lock().await;
        w.append(&batch)?;
        w.acknowledge(seq);
        total_samples += batch.samples.len() as u64;
    }

    // Final flush to stable storage, then report.
    let mut w = wal.lock().await;
    w.sync()?;
    info!(
        samples = total_samples,
        written_seq = w.written_seq(),
        acked_seq = w.acknowledged_seq(),
        segments = w.segment_index() + 1,
        "agent stopped"
    );
    Ok(())
}

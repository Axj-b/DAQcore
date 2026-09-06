// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Agent configuration: agent identity, storage, and sampling cadence.
//!
//! Device blocks are intentionally left out of the typed config: they are
//! driven through the driver registry from the raw TOML so any driver type
//! can contribute its own schema.

use std::path::PathBuf;

use daqcore_wal::Durability;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub agent: AgentSection,
    pub storage: StorageSection,
    #[serde(default)]
    pub sampling: SamplingSection,
}

#[derive(Debug, Deserialize)]
pub struct AgentSection {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct StorageSection {
    pub wal_dir: PathBuf,
    #[serde(default = "default_segment_size")]
    pub segment_size_bytes: u64,
    #[serde(default = "default_durability")]
    pub durability: String,
    #[serde(default = "default_fsync_ms")]
    pub fsync_interval_ms: u64,
}

impl StorageSection {
    pub fn durability(&self) -> daqcore_core::Result<Durability> {
        match self.durability.as_str() {
            "sync_all" => Ok(Durability::SyncAll),
            "sync_data" => Ok(Durability::SyncData),
            "none" => Ok(Durability::None),
            "interval" => Ok(Durability::Interval(self.fsync_interval_ms)),
            other => Err(daqcore_core::Error::Config(format!(
                "unknown durability `{other}`"
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SamplingSection {
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

impl Default for SamplingSection {
    fn default() -> Self {
        Self {
            interval_ms: default_interval_ms(),
            batch_size: default_batch_size(),
        }
    }
}

fn default_segment_size() -> u64 {
    64 * 1024 * 1024
}

fn default_durability() -> String {
    "sync_data".to_string()
}

fn default_fsync_ms() -> u64 {
    1000
}

fn default_interval_ms() -> u64 {
    100
}

fn default_batch_size() -> usize {
    100
}

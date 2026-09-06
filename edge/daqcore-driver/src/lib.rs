// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Pluggable hardware driver engine: the `Driver` trait, a name-based registry,
//! and a synthetic mock bench for offline validation.

mod mock;
mod registry;

pub use mock::{MockBenchConfig, MockChannelConfig, SignalConfig, SyntheticMockBench};
pub use registry::{DriverBuilder, DriverRegistry};

use async_trait::async_trait;
use daqcore_core::{Command, CommandResponse, Result, Sample};
use serde::{Deserialize, Serialize};

/// One channel exposed by a driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    #[serde(default)]
    pub unit: String,
}

/// Static description of a driver: what it is called and which channels it exposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverMetadata {
    pub id: String,
    pub kind: String,
    pub channels: Vec<ChannelInfo>,
}

/// The contract every hardware driver implements.
///
/// Implementations are async so slow device I/O never blocks the sampling loop.
/// All drivers must be `Send + Sync` so the agent can own them across tasks.
#[async_trait]
pub trait Driver: Send + Sync {
    /// Open the device, handshake, and enter a ready state.
    async fn connect(&mut self) -> Result<()>;

    /// Read one cycle of samples from the device.
    async fn sample(&mut self) -> Result<Vec<Sample>>;

    /// Send a command and await its acknowledgement.
    async fn send_command(&mut self, cmd: Command) -> Result<CommandResponse>;

    /// Describe the driver for discovery and configuration.
    fn metadata(&self) -> DriverMetadata;
}

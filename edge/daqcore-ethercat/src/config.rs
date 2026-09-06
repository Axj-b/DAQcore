// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! EtherCAT master configuration: interface, cycle time, and PDO channel map.

use serde::{Deserialize, Serialize};

/// Whether a process-data channel is an input or an output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Input,
    Output,
}

/// One PDO channel exposed to the rest of DAQcore.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    pub direction: Direction,
    #[serde(default)]
    pub unit: String,
    /// Value latched on the bus when communication is lost (outputs only).
    #[serde(default)]
    pub safe_value: f64,
}

/// Configuration for the master and its cyclic engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EthercatConfig {
    /// Network interface name, e.g. `"eth0"` or `"enp1s0"`.
    pub ifname: String,
    /// Cycle time in microseconds (default 1000 = 1 kHz).
    #[serde(default = "default_cycle_us")]
    pub cycle_us: u64,
    /// Slave watchdog = `cycle_us * watchdog_factor`. Headroom absorbs hiccups.
    #[serde(default = "default_watchdog_factor")]
    pub watchdog_factor: u32,
    /// Pin the cyclic thread to this CPU core (best-effort, Linux only).
    #[serde(default)]
    pub pin_core: Option<usize>,
    /// PDO channel map.
    #[serde(default)]
    pub channels: Vec<Channel>,
}

impl EthercatConfig {
    /// Watchdog timeout in microseconds.
    pub fn watchdog_us(&self) -> u64 {
        self.cycle_us.max(1) * self.watchdog_factor.max(1) as u64
    }
}

fn default_cycle_us() -> u64 {
    1000
}

fn default_watchdog_factor() -> u32 {
    20
}

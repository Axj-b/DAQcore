// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Shared state between the cyclic thread and the `Driver` API.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, RwLock};

use crate::state::BusState;

/// Health + counters for the cyclic engine.
#[derive(Debug, Clone)]
pub struct Status {
    pub state: BusState,
    /// True when the last exchange failed or the bus is not in OP.
    pub stale: bool,
    pub last_error: Option<String>,
    pub cycles: u64,
    pub missed_cycles: u64,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            state: BusState::Init,
            stale: true,
            last_error: None,
            cycles: 0,
            missed_cycles: 0,
        }
    }
}

/// State shared between the cyclic thread and the driver.
#[derive(Default)]
pub struct Shared {
    /// Latest input process image (written by the cyclic thread, read by `sample`).
    pub inputs: RwLock<HashMap<String, f64>>,
    /// Pending output process image (written by `send_command`, read by the thread).
    pub outputs: RwLock<HashMap<String, f64>>,
    /// Engine status.
    pub status: Mutex<Status>,
    /// Set to stop the cyclic thread.
    pub shutdown: AtomicBool,
}

impl Shared {
    pub fn is_stale(&self) -> bool {
        self.status.lock().unwrap().stale
    }
}

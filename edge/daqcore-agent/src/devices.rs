// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Runtime device registry: the live set of drivers, shared between the
//! sampling loop and the control plane so devices can be listed, added, read,
//! and written at runtime.

use std::collections::HashMap;
use std::sync::Arc;

use daqcore_driver::{Driver, DriverMetadata};
use tokio::sync::Mutex;

/// A driver behind an async mutex, so it can be sampled by the loop and
/// driven manually by the API.
pub type SharedDriver = Arc<Mutex<Box<dyn Driver>>>;

pub struct DeviceEntry {
    pub metadata: DriverMetadata,
    pub driver: SharedDriver,
    pub connected: bool,
}

/// Order-preserving map of device id -> live driver.
pub struct DeviceManager {
    entries: HashMap<String, DeviceEntry>,
    order: Vec<String>,
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
        }
    }

    pub fn insert(&mut self, id: String, metadata: DriverMetadata, driver: SharedDriver) {
        if !self.entries.contains_key(&id) {
            self.order.push(id.clone());
        }
        self.entries.insert(
            id,
            DeviceEntry {
                metadata,
                driver,
                connected: true,
            },
        );
    }

    pub fn get(&self, id: &str) -> Option<&DeviceEntry> {
        self.entries.get(id)
    }

    /// Snapshot of all metadata in insertion order.
    pub fn metadata(&self) -> Vec<DriverMetadata> {
        self.order
            .iter()
            .filter_map(|id| self.entries.get(id).map(|e| e.metadata.clone()))
            .collect()
    }

    /// Snapshot of (id, driver) handles for the sampling loop.
    pub fn shared_drivers(&self) -> Vec<(String, SharedDriver)> {
        self.order
            .iter()
            .filter_map(|id| self.entries.get(id).map(|e| (id.clone(), e.driver.clone())))
            .collect()
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}

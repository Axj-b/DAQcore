// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Name-based driver registry. Config addresses a device by a `type` string;
//! the registry maps that string to a constructor.

use std::collections::HashMap;

use daqcore_core::{Error, Result};

use crate::{Driver, MockBenchConfig, SyntheticMockBench};

/// A constructor that builds a driver from its TOML config table.
pub type DriverBuilder = Box<dyn Fn(&toml::Table) -> Result<Box<dyn Driver>> + Send + Sync>;

/// Maps driver type names to builders.
#[derive(Default)]
pub struct DriverRegistry {
    builders: HashMap<&'static str, DriverBuilder>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a driver type under `name`.
    pub fn register(&mut self, name: &'static str, builder: DriverBuilder) {
        self.builders.insert(name, builder);
    }

    /// Build a driver instance from a raw TOML table, keyed by its `type`.
    pub fn build(&self, name: &str, config: &toml::Table) -> Result<Box<dyn Driver>> {
        let builder = self
            .builders
            .get(name)
            .ok_or_else(|| Error::Driver(format!("unknown driver type `{name}`")))?;
        builder(config)
    }

    /// A registry pre-populated with the built-in `mock` bench driver.
    pub fn with_mock(mut self) -> Self {
        self.register(
            "mock",
            Box::new(|table| {
                let cfg: MockBenchConfig = toml::Value::Table(table.clone())
                    .try_into()
                    .map_err(|e| Error::Config(format!("mock driver config: {e}")))?;
                Ok(Box::new(SyntheticMockBench::new(cfg)) as Box<dyn Driver>)
            }),
        );
        self
    }
}

// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! The master abstraction: whatever actually drives the EtherCAT frame exchange.

use std::collections::HashMap;

use daqcore_core::Result;

use crate::config::Channel;
use crate::state::BusState;

/// A backend that drives the cyclic EtherCAT frame exchange.
///
/// Implementations are blocking (SOEM is a blocking C library); the cyclic
/// engine runs them on a dedicated thread.
pub trait EthercatMaster: Send + Sync {
    /// Initialize the bus and bring all slaves to `Op`.
    fn start(&mut self) -> Result<()>;

    /// Exchange one cycle: apply `outputs`, read back `inputs`.
    fn exchange(
        &mut self,
        outputs: &HashMap<String, f64>,
        inputs: &mut HashMap<String, f64>,
    ) -> Result<()>;

    /// Current bus state.
    fn state(&self) -> BusState;

    /// Stop the bus (to a safe state).
    fn stop(&mut self) -> Result<()>;

    /// The configured PDO channel layout.
    fn channels(&self) -> &[Channel];
}

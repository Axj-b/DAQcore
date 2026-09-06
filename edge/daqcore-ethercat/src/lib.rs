// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! EtherCAT driver for DAQcore.
//!
//! EtherCAT is a cyclic real-time fieldbus: a **master** (this crate) drives a
//! fixed-rate frame exchange with **slaves** (e.g. a Beckhoff EK1100 bus
//! coupler and its terminals). Process data (PDOs) are exchanged every cycle,
//! not on demand.
//!
//! Architecture:
//!
//! ```text
//! EthercatDriver (Driver trait)
//!   └── CyclicEngine (dedicated thread, fixed cycle rate)
//!         └── EthercatMaster  (trait)
//!               ├── SimMaster   (always available; development/testing)
//!               └── SoemMaster  (feature "soem"; Linux + libsoem)
//! ```
//!
//! Safeguards built in:
//! - configurable cycle time + slave watchdog headroom (absorb jitter);
//! - a dedicated cyclic thread (pinned to a core on Linux, best-effort);
//! - a state supervisor that flags stale data and auto-recovers the bus to OP;
//! - a fail-safe output latch applied whenever the bus is not in OP.

mod config;
mod cycle;
mod driver;
mod master;
mod shared;
mod sim;
#[cfg(feature = "soem")]
mod soem;
mod state;

pub use config::{Channel, Direction, EthercatConfig};
pub use driver::EthercatDriver;
pub use master::EthercatMaster;
pub use sim::SimMaster;
#[cfg(feature = "soem")]
pub use soem::SoemMaster;
pub use state::BusState;

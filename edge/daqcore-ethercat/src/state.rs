// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! EtherCAT bus states.

/// State of the EtherCAT bus / master.
///
/// EtherCAT slaves move through a state machine; `Op` is the only state in
/// which cyclic process-data exchange happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusState {
    Init,
    PreOp,
    SafeOp,
    Op,
    Error,
}

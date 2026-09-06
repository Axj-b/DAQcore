// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Simulation master: a software stand-in for the bus, for development and tests.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use daqcore_core::{Error, Result};

use crate::config::{Channel, Direction};
use crate::master::EthercatMaster;
use crate::state::BusState;

/// A fake EtherCAT bus.
///
/// Input channels produce a monotonically increasing value; output channels
/// echo the applied output back as an input (so tests can observe writes).
pub struct SimMaster {
    channels: Vec<Channel>,
    state: BusState,
    counter: f64,
    /// Number of injected exchange failures remaining.
    fail_trigger: Arc<AtomicU64>,
}

impl SimMaster {
    pub fn new(channels: Vec<Channel>) -> Self {
        Self {
            channels,
            state: BusState::Init,
            counter: 0.0,
            fail_trigger: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Cause the next `n` exchanges to fail (simulates a bus hiccup).
    pub fn inject_failures(&self, n: u64) {
        self.fail_trigger.store(n, Ordering::SeqCst);
    }

    /// A handle to inject failures after this master has been moved into a thread.
    pub fn fail_handle(&self) -> Arc<AtomicU64> {
        self.fail_trigger.clone()
    }
}

impl EthercatMaster for SimMaster {
    fn start(&mut self) -> Result<()> {
        self.state = BusState::Op;
        Ok(())
    }

    fn exchange(
        &mut self,
        outputs: &HashMap<String, f64>,
        inputs: &mut HashMap<String, f64>,
    ) -> Result<()> {
        if self.fail_trigger.load(Ordering::SeqCst) > 0 {
            self.fail_trigger.fetch_sub(1, Ordering::SeqCst);
            self.state = BusState::Error;
            return Err(Error::Driver("simulated bus fault".into()));
        }

        self.state = BusState::Op;
        self.counter += 1.0;
        for ch in &self.channels {
            let value = match ch.direction {
                Direction::Input => self.counter,
                Direction::Output => outputs.get(&ch.name).copied().unwrap_or(ch.safe_value),
            };
            inputs.insert(ch.name.clone(), value);
        }
        Ok(())
    }

    fn state(&self) -> BusState {
        self.state
    }

    fn stop(&mut self) -> Result<()> {
        self.state = BusState::Init;
        Ok(())
    }

    fn channels(&self) -> &[Channel] {
        &self.channels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(name: &str, dir: Direction) -> Channel {
        Channel {
            name: name.into(),
            direction: dir,
            unit: String::new(),
            safe_value: 0.0,
        }
    }

    #[test]
    fn exchange_produces_inputs_and_echoes_outputs() {
        let mut m = SimMaster::new(vec![
            ch("di.0", Direction::Input),
            ch("do.0", Direction::Output),
        ]);
        m.start().unwrap();
        assert_eq!(m.state(), BusState::Op);

        let outputs = HashMap::from([("do.0".to_string(), 1.5)]);
        let mut inputs = HashMap::new();
        m.exchange(&outputs, &mut inputs).unwrap();

        assert!(inputs["di.0"] > 0.0);
        assert_eq!(inputs["do.0"], 1.5);
    }

    #[test]
    fn injected_failure_errors_and_recovers() {
        let mut m = SimMaster::new(vec![ch("di.0", Direction::Input)]);
        m.start().unwrap();

        m.inject_failures(1);
        let mut inputs = HashMap::new();
        assert!(m.exchange(&HashMap::new(), &mut inputs).is_err());
        assert_eq!(m.state(), BusState::Error);

        // Next exchange succeeds again.
        m.exchange(&HashMap::new(), &mut inputs).unwrap();
        assert_eq!(m.state(), BusState::Op);
    }
}

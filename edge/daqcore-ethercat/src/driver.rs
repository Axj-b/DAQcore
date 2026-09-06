// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! The `Driver` implementation that exposes the EtherCAT process image to the
//! rest of DAQcore.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;

use async_trait::async_trait;
use daqcore_core::{Command, CommandResponse, Error, Result, Sample, Timestamp, Value};
use daqcore_driver::{ChannelInfo, Driver, DriverMetadata};

use crate::config::{Direction, EthercatConfig};
use crate::cycle;
use crate::master::EthercatMaster;
use crate::shared::Shared;

/// An EtherCAT device as seen by the agent.
pub struct EthercatDriver {
    name: String,
    channels: Vec<ChannelInfo>,
    cfg: EthercatConfig,
    master: Option<Box<dyn EthercatMaster>>,
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl EthercatDriver {
    pub fn new(
        name: impl Into<String>,
        cfg: EthercatConfig,
        master: Box<dyn EthercatMaster>,
    ) -> Self {
        let channels = cfg
            .channels
            .iter()
            .filter(|c| c.direction == Direction::Input)
            .map(|c| ChannelInfo {
                id: c.name.clone(),
                unit: c.unit.clone(),
            })
            .collect();
        Self {
            name: name.into(),
            channels,
            cfg,
            master: Some(master),
            shared: Arc::new(Shared::default()),
            handle: None,
        }
    }
}

#[async_trait]
impl Driver for EthercatDriver {
    async fn connect(&mut self) -> Result<()> {
        if self.handle.is_some() {
            return Ok(());
        }
        self.shared.shutdown.store(false, Ordering::Release);
        let master = self
            .master
            .take()
            .ok_or_else(|| Error::Driver("ethercat master already started".into()))?;
        let handle = cycle::spawn(master, self.shared.clone(), self.cfg.clone());
        self.handle = Some(handle);
        Ok(())
    }

    async fn sample(&mut self) -> Result<Vec<Sample>> {
        if self.shared.is_stale() {
            return Err(Error::Driver("ethercat bus not in OP (stale data)".into()));
        }
        let inputs = self.shared.inputs.read().unwrap();
        let ts = now_nanos();
        let mut out = Vec::with_capacity(self.channels.len());
        for ch in &self.channels {
            if let Some(v) = inputs.get(&ch.id) {
                out.push(Sample::new(ts, ch.id.clone(), Value::F64(*v)));
            }
        }
        Ok(out)
    }

    async fn send_command(&mut self, cmd: Command) -> Result<CommandResponse> {
        let value = cmd.args.first().and_then(|v| v.as_f64()).ok_or_else(|| {
            Error::Driver(format!("output `{}` requires a numeric value", cmd.op))
        })?;
        self.shared
            .outputs
            .write()
            .unwrap()
            .insert(cmd.op.clone(), value);
        Ok(CommandResponse::ok(format!("set {} = {}", cmd.op, value)))
    }

    fn metadata(&self) -> DriverMetadata {
        DriverMetadata {
            id: self.name.clone(),
            kind: "ethercat".to_string(),
            channels: self.channels.clone(),
        }
    }
}

impl Drop for EthercatDriver {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn now_nanos() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as Timestamp)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Channel;
    use crate::sim::SimMaster;
    use std::time::Duration;

    fn channel(name: &str, dir: Direction) -> Channel {
        Channel {
            name: name.into(),
            direction: dir,
            unit: String::new(),
            safe_value: 0.0,
        }
    }

    fn test_cfg() -> EthercatConfig {
        EthercatConfig {
            ifname: "sim".into(),
            cycle_us: 1000,
            watchdog_factor: 20,
            pin_core: None,
            channels: vec![
                channel("di.0", Direction::Input),
                channel("do.0", Direction::Output),
            ],
        }
    }

    #[tokio::test]
    async fn sample_and_command_roundtrip() {
        let cfg = test_cfg();
        let master = SimMaster::new(cfg.channels.clone());
        let mut driver = EthercatDriver::new("ek1100", cfg, Box::new(master));
        driver.connect().await.unwrap();

        std::thread::sleep(Duration::from_millis(20));

        let samples = driver.sample().await.unwrap();
        assert!(!samples.is_empty());
        assert_eq!(samples[0].channel, "di.0");

        let resp = driver
            .send_command(Command {
                device: "ek1100".into(),
                op: "do.0".into(),
                args: vec![Value::F64(5.0)],
            })
            .await
            .unwrap();
        assert!(resp.ok);
    }

    #[tokio::test]
    async fn fault_recovers_to_op() {
        let cfg = test_cfg();
        let master = SimMaster::new(cfg.channels.clone());
        let fail = master.fail_handle();
        let mut driver = EthercatDriver::new("ek1100", cfg, Box::new(master));
        driver.connect().await.unwrap();

        std::thread::sleep(Duration::from_millis(20));
        assert!(driver.sample().await.is_ok());

        // Inject one fault, then wait for the engine to recover to OP.
        fail.store(1, Ordering::SeqCst);
        let mut recovered = false;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(20));
            if driver.sample().await.is_ok() {
                recovered = true;
                break;
            }
        }
        assert!(recovered, "driver did not recover from injected fault");
    }
}

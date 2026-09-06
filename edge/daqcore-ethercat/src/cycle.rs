// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! The cyclic engine: a dedicated thread that drives the frame exchange at a
//! fixed rate, keeps the process image fresh, flags stale data, applies the
//! fail-safe latch, and auto-recovers the bus after a fault.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tracing::{error, warn};

use crate::config::{Channel, Direction, EthercatConfig};
use crate::master::EthercatMaster;
use crate::shared::Shared;
use crate::state::BusState;

/// Spawn the cyclic thread. Returns its join handle.
pub fn spawn(
    master: Box<dyn EthercatMaster>,
    shared: Arc<Shared>,
    cfg: EthercatConfig,
) -> std::thread::JoinHandle<()> {
    let channels = master.channels().to_vec();
    std::thread::Builder::new()
        .name("daqcore-ethercat".to_string())
        .spawn(move || run_loop(master, shared, cfg, channels))
        .expect("failed to spawn ethercat cycle thread")
}

fn run_loop(
    mut master: Box<dyn EthercatMaster>,
    shared: Arc<Shared>,
    cfg: EthercatConfig,
    channels: Vec<Channel>,
) {
    if let Some(core) = cfg.pin_core {
        set_affinity(core);
    }

    // Bring the bus to OP; if that fails, mark stale and give up (nothing to cycle).
    if let Err(e) = master.start() {
        let mut s = shared.status.lock().unwrap();
        s.state = BusState::Error;
        s.stale = true;
        s.last_error = Some(e.to_string());
        return;
    }

    let safe_outputs = build_safe_outputs(&channels);
    let cycle = Duration::from_micros(cfg.cycle_us.max(50));
    let mut next = Instant::now();
    let mut backoff = Duration::from_millis(100);

    while !shared.shutdown.load(Ordering::Acquire) {
        // Fixed-deadline scheduling: if we're already past the next slot, count
        // a miss and resync (rather than busy-catching-up).
        let now = Instant::now();
        if now > next {
            shared.status.lock().unwrap().missed_cycles += 1;
            next = now;
        }

        // Fail-safe: only apply user outputs while the bus is actually in OP.
        let in_op = master.state() == BusState::Op;
        let effective = if in_op {
            shared.outputs.read().unwrap().clone()
        } else {
            safe_outputs.clone()
        };

        let mut inputs = HashMap::new();
        match master.exchange(&effective, &mut inputs) {
            Ok(()) => {
                let st = master.state();
                {
                    let mut s = shared.status.lock().unwrap();
                    s.cycles += 1;
                    s.state = st;
                    s.stale = st != BusState::Op;
                    s.last_error = None;
                }
                *shared.inputs.write().unwrap() = inputs;
                backoff = Duration::from_millis(100);
            }
            Err(e) => {
                warn!("ethercat cycle failed: {e}");
                {
                    let mut s = shared.status.lock().unwrap();
                    s.state = BusState::Error;
                    s.stale = true;
                    s.missed_cycles += 1;
                    s.last_error = Some(e.to_string());
                }
                // Auto-recovery: back off, then re-drive the bus to OP.
                std::thread::sleep(backoff);
                let _ = master.stop();
                match master.start() {
                    Ok(()) => backoff = Duration::from_millis(100),
                    Err(e) => {
                        error!("ethercat recovery failed: {e}");
                        backoff = (backoff * 2).min(Duration::from_secs(5));
                    }
                }
            }
        }

        next += cycle;
        let remaining = next.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            std::thread::sleep(remaining);
        }
    }

    let _ = master.stop();
}

/// Output channels -> their fail-safe values.
fn build_safe_outputs(channels: &[Channel]) -> HashMap<String, f64> {
    channels
        .iter()
        .filter(|c| c.direction == Direction::Output)
        .map(|c| (c.name.clone(), c.safe_value))
        .collect()
}

#[cfg(target_os = "linux")]
fn set_affinity(core: usize) {
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(core, &mut set);
        let size = std::mem::size_of::<libc::cpu_set_t>();
        libc::sched_setaffinity(0, size, &set);
    }
}

#[cfg(not(target_os = "linux"))]
fn set_affinity(_core: usize) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::SimMaster;

    fn input(name: &str) -> Channel {
        Channel {
            name: name.into(),
            direction: Direction::Input,
            unit: String::new(),
            safe_value: 0.0,
        }
    }

    fn output(name: &str, safe: f64) -> Channel {
        Channel {
            name: name.into(),
            direction: Direction::Output,
            unit: String::new(),
            safe_value: safe,
        }
    }

    fn cfg() -> EthercatConfig {
        EthercatConfig {
            ifname: "sim".into(),
            cycle_us: 1000,
            watchdog_factor: 20,
            pin_core: None,
            channels: vec![input("di.0"), output("do.0", 3.0)],
        }
    }

    #[test]
    fn cycles_and_publishes_inputs() {
        let shared = Arc::new(Shared::default());
        let master = SimMaster::new(cfg().channels);
        let handle = spawn(Box::new(master), shared.clone(), cfg());

        std::thread::sleep(Duration::from_millis(30));

        let status = shared.status.lock().unwrap().clone();
        assert_eq!(status.state, BusState::Op);
        assert!(!status.stale);
        assert!(status.cycles > 5);

        let inputs = shared.inputs.read().unwrap();
        assert!(inputs.contains_key("di.0"));

        shared.shutdown.store(true, Ordering::Release);
        handle.join().unwrap();
    }
}

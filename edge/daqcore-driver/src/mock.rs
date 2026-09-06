// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Synthetic mock bench: configurable multi-channel signal generators for
//! offline validation without physical hardware.

use std::f64::consts::PI;
use std::time::Instant;

use async_trait::async_trait;
use daqcore_core::{Command, CommandResponse, Result, Sample, Timestamp, Value};
use fastrand::Rng;
use serde::{Deserialize, Serialize};

use crate::{Driver, DriverMetadata};

/// The waveform a mock channel generates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SignalConfig {
    #[serde(rename = "sine")]
    Sine {
        amplitude: f64,
        offset: f64,
        frequency_hz: f64,
    },
    #[serde(rename = "square")]
    Square {
        low: f64,
        high: f64,
        frequency_hz: f64,
    },
    #[serde(rename = "thermal")]
    Thermal {
        ambient: f64,
        amplitude: f64,
        period_secs: f64,
    },
    #[serde(rename = "dc_sweep")]
    DcSweep {
        start: f64,
        end: f64,
        period_secs: f64,
    },
    #[serde(rename = "noise")]
    Noise { mean: f64, stddev: f64 },
    #[serde(rename = "drift")]
    Drift { start: f64, rate_per_sec: f64 },
}

impl SignalConfig {
    /// Compute the waveform value at time `t` (seconds since start).
    fn eval(&self, t: f64, rng: &mut Rng) -> f64 {
        match self {
            SignalConfig::Sine {
                amplitude,
                offset,
                frequency_hz,
            } => offset + amplitude * (2.0 * PI * frequency_hz * t).sin(),
            SignalConfig::Square {
                low,
                high,
                frequency_hz,
            } => {
                if (2.0 * PI * frequency_hz * t).sin() >= 0.0 {
                    *high
                } else {
                    *low
                }
            }
            SignalConfig::Thermal {
                ambient,
                amplitude,
                period_secs,
            } => ambient + amplitude * (2.0 * PI * t / period_secs).sin(),
            SignalConfig::DcSweep {
                start,
                end,
                period_secs,
            } => {
                let phase = (t / period_secs).fract();
                let tri = if phase < 0.5 {
                    2.0 * phase
                } else {
                    2.0 - 2.0 * phase
                };
                start + (end - start) * tri
            }
            SignalConfig::Noise { mean, stddev } => mean + stddev * gaussian(rng),
            SignalConfig::Drift {
                start,
                rate_per_sec,
            } => start + rate_per_sec * t,
        }
    }
}

/// One generated channel in the mock bench.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockChannelConfig {
    pub id: String,
    #[serde(default)]
    pub unit: String,
    pub signal: SignalConfig,
}

/// Configuration for a whole synthetic bench (one or more channels).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MockBenchConfig {
    #[serde(default = "default_id")]
    pub id: String,
    #[serde(default)]
    pub channels: Vec<MockChannelConfig>,
}

fn default_id() -> String {
    "mock".to_string()
}

impl Default for MockBenchConfig {
    fn default() -> Self {
        Self {
            id: default_id(),
            channels: Vec::new(),
        }
    }
}

/// Deterministically seeded source of noise.
const SEED: u64 = 0x5eed_da7c_c0de;

/// The synthetic bench itself.
pub struct SyntheticMockBench {
    config: MockBenchConfig,
    start: Option<Instant>,
    rng: Rng,
}

impl SyntheticMockBench {
    pub fn new(config: MockBenchConfig) -> Self {
        Self {
            config,
            start: None,
            rng: Rng::with_seed(SEED),
        }
    }

    fn elapsed_secs(&self) -> f64 {
        self.start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0)
    }
}

#[async_trait]
impl Driver for SyntheticMockBench {
    async fn connect(&mut self) -> Result<()> {
        self.start = Some(Instant::now());
        Ok(())
    }

    async fn sample(&mut self) -> Result<Vec<Sample>> {
        // All channels share one timestamp per cycle.
        let t = self.elapsed_secs();
        let ts = now_nanos();
        let mut out = Vec::with_capacity(self.config.channels.len());
        for ch in &self.config.channels {
            let v = ch.signal.eval(t, &mut self.rng);
            out.push(Sample::new(ts, ch.id.clone(), Value::F64(v)));
        }
        Ok(out)
    }

    async fn send_command(&mut self, cmd: Command) -> Result<CommandResponse> {
        Ok(CommandResponse::ok(format!("ack {}", cmd.op)))
    }

    fn metadata(&self) -> DriverMetadata {
        DriverMetadata {
            id: self.config.id.clone(),
            kind: "mock".to_string(),
            channels: self.config.channels.iter().map(|c| c.id.clone()).collect(),
        }
    }
}

fn now_nanos() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as Timestamp)
        .unwrap_or(0)
}

/// Standard-normal sample via Box-Muller.
fn gaussian(rng: &mut Rng) -> f64 {
    let u1 = rng.f64().max(1e-12);
    let u2 = rng.f64();
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_is_bounded() {
        let sig = SignalConfig::Sine {
            amplitude: 5.0,
            offset: 10.0,
            frequency_hz: 1.0,
        };
        let mut rng = Rng::with_seed(1);
        for i in 0..100 {
            let v = sig.eval(i as f64 / 20.0, &mut rng);
            assert!((5.0..=15.0).contains(&v), "value {v} out of range");
        }
    }

    #[test]
    fn square_switches() {
        let sig = SignalConfig::Square {
            low: 0.0,
            high: 1.0,
            frequency_hz: 1.0,
        };
        let mut rng = Rng::with_seed(1);
        assert_eq!(sig.eval(0.1, &mut rng), 1.0);
        assert_eq!(sig.eval(0.6, &mut rng), 0.0);
    }

    #[test]
    fn toml_roundtrip() {
        let toml_str = r#"
id = "chamber"
[[channels]]
id = "chamber.temp"
unit = "degC"
[channels.signal]
kind = "thermal"
ambient = 40.0
amplitude = 20.0
period_secs = 60.0
"#;
        let cfg: MockBenchConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.id, "chamber");
        assert_eq!(cfg.channels.len(), 1);
        assert_eq!(cfg.channels[0].id, "chamber.temp");
    }
}

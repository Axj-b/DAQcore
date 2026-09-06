// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! Shared core types for DAQcore: samples, batches, values, commands, and errors.

use serde::{Deserialize, Serialize};

/// Nanoseconds since the UNIX epoch.
pub type Timestamp = u64;

/// Identifier for a logical channel (e.g. `"chamber.temp"`).
pub type ChannelId = String;

/// Monotonic sequence number for ordered batches and WAL cursors.
pub type Seq = u64;

/// The kind of data a channel carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelType {
    Float,
    Int,
    Bool,
    String,
}

/// A single scalar reading. Stored as a tagged union so heterogeneous
/// channels can share one batch without boxing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    F64(f64),
    I64(i64),
    Bool(bool),
    Str(String),
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::F64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(v) => Some(v),
            _ => None,
        }
    }

    pub fn channel_type(&self) -> ChannelType {
        match self {
            Value::F64(_) => ChannelType::Float,
            Value::I64(_) => ChannelType::Int,
            Value::Bool(_) => ChannelType::Bool,
            Value::Str(_) => ChannelType::String,
        }
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::F64(v)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::I64(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}

/// One timestamped reading on one channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub ts: Timestamp,
    pub channel: ChannelId,
    pub value: Value,
}

impl Sample {
    pub fn new(ts: Timestamp, channel: impl Into<ChannelId>, value: impl Into<Value>) -> Self {
        Self {
            ts,
            channel: channel.into(),
            value: value.into(),
        }
    }
}

/// A contiguous group of samples sharing one sequence number.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleBatch {
    pub seq: Seq,
    pub samples: Vec<Sample>,
}

impl SampleBatch {
    pub fn new(seq: Seq) -> Self {
        Self {
            seq,
            samples: Vec::new(),
        }
    }

    pub fn push(&mut self, sample: Sample) {
        self.samples.push(sample);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Coarse health state of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceStatus {
    Connected,
    Disconnected,
    Faulted,
}

/// A device command issued by a script, pipeline, or remote caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub device: String,
    pub op: String,
    #[serde(default)]
    pub args: Vec<Value>,
}

impl Command {
    pub fn new(device: impl Into<String>, op: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            op: op.into(),
            args: Vec::new(),
        }
    }
}

/// Acknowledgement of a command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandResponse {
    pub ok: bool,
    pub message: String,
}

impl CommandResponse {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

/// Crate-wide error type shared across the DAQcore workspace.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("encode error: {0}")]
    Encode(String),
    #[error("decode error: {0}")]
    Decode(String),
    #[error("driver error: {0}")]
    Driver(String),
    #[error("wal error: {0}")]
    Wal(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("not connected")]
    NotConnected,
}

pub type Result<T> = std::result::Result<T, Error>;

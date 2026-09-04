# DAQcore — Architecture & Design Document

> A high-performance, edge-to-cloud framework for industrial test benches, hardware
> compliance labs, and durability testing.
>
> DAQcore turns chaotic raw sensor data into clear, actionable engineering insight.

Related documents:

- [`platform-strategy.md`](platform-strategy.md) — open-core business model & editions
- [`extensibility.md`](extensibility.md) — deployment, drivers, plugins & custom devices
- [`radar-test-station.md`](radar-test-station.md) — worked example: a real radar test station
- [`calibration-station.md`](calibration-station.md) — worked example: a metrology/calibration station
- [`sample-management.md`](sample-management.md) — tracking every unit under test: lifecycle, software & calibration state, graveyard
- [`remote-access.md`](remote-access.md) — Secure Shell over the cloud: audited SSH via the agent's outbound tunnel
- [`roadmap.md`](roadmap.md) — prioritized feature backlog (P0 → P3)
- [`compliance.md`](compliance.md) — CRA, GDPR, and certification (CE out of scope)

---

## 1. Vision

DAQcore replaces clunky, laggy web software and per-machine custom code with a modern,
reliable "DAQ-Core" platform:

- **Edge Agent (Rust)** — a blazing-fast, lightweight binary running on a Raspberry Pi or
  industrial PC. It handles local hardware communication (DAQ devices, power supplies,
  PLCs), performs local offline buffering, and guarantees zero memory-leak reliability.
- **Cloud & Dashboard Layer** — a real-time telemetry pipeline and multi-tenant dashboard
  that lets engineers monitor, log, and remotely control long-running durability tests
  across global deployments from a single portal.

---

## 2. Design Goals

| Goal | Target |
| --- | --- |
| Zero memory leaks | Deterministic allocation; bounded ring buffers; arena-style pre-allocated batches |
| Offline resilience | Local crash-safe Write-Ahead Log (WAL) with backpressure and catch-up playback |
| Fast edge-to-cloud transport | Binary streaming (gRPC / QUIC + Protobuf), with optional binary WebSocket for live debug |
| Sub-millisecond jitter | Tokio async runtime + dedicated blocking driver workers |
| Long-running durability tests | Graceful shutdown, flush-to-disk guarantees, auto-reconnect |

---

## 3. System Overview

```
┌────────────────────────────────────────────────────────────────────────────┐
│                              DAQcore Edge Agent                            │
│                                                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                    Pluggable Driver Engine                           │  │
│  │  ┌───────────────┐ ┌───────────────┐ ┌───────────────┐ ┌──────────┐  │  │
│  │  │  SCPI / VISA  │ │  Modbus TCP   │ │    OPC UA     │ │Synthetic │  │  │
│  │  │ (Keysight/DMM)│ │   & Modbus RTU│ │  Client Node  │ │Mock Bench│  │  │
│  │  └───────┬───────┘ └───────┬───────┘ └───────┬───────┘ └────┬─────┘  │  │
│  └──────────┼─────────────────┼─────────────────┼──────────────┼────────┘  │
│             └─────────────────┼─────────────────┘              │           │
│                               ▼                                │           │
│  ┌─────────────────────────────────────────────────────────────┼────────┐  │
│  │         High-Throughput Lockless Channel (crossbeam/flume)  │        │  │
│  └────────────────────────────┬────────────────────────────────┴────────┘  │
│                               ▼                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │          Local Crash-Safe Write-Ahead Log (WAL) Disk Buffer          │  │
│  │    • Append-only binary segments with CRC32 verification             │  │
│  │    • Configurable max disk size & ring-style retention               │  │
│  │    • Automatic head/tail tracking for offline backpressure           │  │
│  └────────────────────────────┬─────────────────────────────────────────┘  │
│                               ▼                                            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │                      Egress & Dispatcher Layer                       │  │
│  │  ┌─────────────────────────────────┐ ┌────────────────────────────┐ │  │
│  │  │ Ultra-Fast gRPC / QUIC Stream   │ │ Embedded HTTP/WS Live Debug│ │  │
│  │  │ (Protobuf telemetry & commands) │ │ (Local Axum server & API)  │ │  │
│  │  └────────────────┬────────────────┘ └──────────────┬─────────────┘ │  │
│  └───────────────────┼─────────────────────────────────┼────────────────┘  │
└──────────────────────┼─────────────────────────────────┼───────────────────┘
                       ▼                                 ▼
         [ Cloud Ingest / Central ]             [ Local Tech Dashboard ]
```

---

## 4. Core Modules

### 4.1 Pluggable Hardware Driver Engine (`daqcore-driver`)

**`Driver` trait contract:**

```rust
trait Driver {
    async fn connect(&mut self) -> Result<()>;
    async fn sample(&mut self) -> Result<Vec<Sample>>;
    async fn send_command(&mut self, cmd: Command) -> Result<CommandResponse>;
    fn metadata(&self) -> DriverMetadata;
}
```

**Protocol implementations:**

1. **SCPI / VISA (TCP & Serial)** — text-based instrument control
   (`*IDN?`, `MEAS:VOLT:DC?`, `:SOUR:VOLT 12.0`). Handles line framing, timeouts, and
   query-response cycles.
2. **Modbus TCP & RTU** — register polling (`HoldingRegisters`, `InputRegisters`, `Coils`)
   with configurable conversion specs (IEEE 754 Float32/64, scaled INT16/32, Big/Little
   Endian byte swapping).
3. **OPC UA** — polling and subscription-based node monitor connecting to PLC server
   endpoints (`opc.tcp://...`).
4. **Synthetic / Mock Test Bench** — configurable multi-channel generator (thermal chamber
   profile, sine/square wave, DC load sweep, Gaussian noise, and drift) for realistic
   offline validation.

### 4.2 Local Crash-Safe WAL Buffer (`daqcore-wal`)

- **Normalization pipeline** — samples from all drivers are normalized into `SampleBatch`
  records before hitting disk/network.
- **Append-only segmented WAL** — pre-allocated segment files (e.g. 64 MB each) with CRC32
  checksums per record; structured fixed-header binary layout to avoid heap fragmentation.
- **Crash-proof sync** — configurable durability (`sync_all`, `sync_data`, interval batch
  `fsync`).
- **Cursor management** — tracks `acknowledged_seq` vs `written_seq` so the agent resumes
  streaming after network dropouts without data loss.

### 4.3 Fast Transport Layer (`daqcore-transport`)

- **gRPC streaming (Protobuf)** — bi-directional stream: Edge pushes batched telemetry;
  Cloud pushes remote commands (abort test, change setpoint, trigger relay). Batch
  compression via Snappy/Zstd for high-frequency metrics.
- **Local Axum HTTP & WebSocket API** — live debugging without cloud connectivity
  (`/api/v1/live`, `/api/v1/health`, `/api/v1/drivers`, `/api/v1/metrics`).

### 4.4 Configuration & Core Runtime (`daqcore-agent`)

- YAML/TOML configuration declaring devices, polling rates (1 Hz – 10 kHz), channel
  mappings, and network endpoints.
- Graceful shutdown handlers (`Ctrl+C`, `SIGTERM`) with flush-to-disk guarantees.

### 4.5 Automation Pipelines (`daqcore-pipeline`)

A declarative, reusable data-processing pipeline ("automation pipe") that runs
deterministically on the edge. Each pipe is a DAG of stages:

```
source ──▶ transform ──▶ condition ──▶ action
```

| Stage | Purpose | Examples |
| --- | --- | --- |
| `source` | Read inputs | channels, points, computed signals |
| `transform` | Shape data | scale/offset, unit convert, filter, window/aggregate, deadband |
| `condition` | Evaluate | threshold, rate-of-change, statistical, expression |
| `action` | React | alert, log, relay trigger, setpoint change, forward to cloud |

**Two authoring surfaces:**

1. **Edge-local** — configure a pipe directly on the device (config file, local UI, or REST
   API). Runs fully offline; no cloud required.
2. **Cloud → fleet** — author and version pipes in the DAQcore Console, then deploy to one
   or many devices across a global fleet, with per-device status, drift detection, and
   rollback. Devices cache the pipe locally so automation keeps running even if the cloud
   drops.

**Guarantees:** deterministic execution, bounded memory, no allocation in the hot path, and
full offline operation. Pipes are versioned artifacts that can be pushed to the marketplace.

**How it runs on the edge:**

The Edge Agent is always streaming samples (drivers → WAL → transport). A pipe is a
declarative config that the agent loads and evaluates continuously *in parallel* with that
stream — it taps the same data but never blocks acquisition. The agent compiles each pipe
into a small DAG once, then a pipe worker subscribes to the declared channels and evaluates
conditions on every incoming sample.

**Three ways to configure a pipe on a device:**

1. **File on disk** — drop `pipelines.yaml` next to the agent config and restart (or
   hot-reload).
2. **Local web UI** — the embedded dashboard at `http://<device-ip>:8080` has a pipe editor.
3. **Local REST API** — `POST /api/v1/pipelines` (the same endpoint the cloud uses to
   deploy to a device).

**Concrete example** (`pipelines.yaml` on the device):

```yaml
pipelines:
  - id: overtemp-guard
    source: { channels: ["chamber.temp", "rail.voltage"] }
    transform:
      - op: window
        fn: mean
        window: 1s
        out: temp_avg
    condition:
      expr: "temp_avg > 90"
    action:
      - type: driver_command    # trip the heater relay locally
        device: heater_relay
        command: open
      - type: alert
        severity: critical
        message: "Chamber overtemp"
      - type: forward           # only if cloud is reachable
```

Runtime flow: the agent compiles the pipe once → a worker subscribes to `chamber.temp`,
buffers 1 s of samples, computes the mean → on every sample it checks `temp_avg > 90` → if
true it sends `open` to the relay driver locally, logs an alert, and (if the cloud is
connected) forwards the event. Steps 1–4 run fully offline.

The same pipe definition runs identically whether written by hand on one device or deployed
from the cloud to an entire fleet.

### 4.6 Test Scripting (`daqcore-script`)

Test scripting is the procedural complement to pipelines. Where a pipeline is *reactive*
(always-on: `temp > 90 → trip relay`), a test script is a *procedural recipe* — a finite,
time-driven sequence of steps that runs a durability/compliance test end-to-end.

**Model** — a declarative YAML script the agent executes deterministically on the edge:

```yaml
test:
  name: thermal-cycle-durability
  version: 3
  parameters:
    v_start: 12
    v_end:   24
    t_high:  85
    t_low:  -40
    cycles:  1000

  steps:
    - set:  { device: psu, channel: voltage, value: 12 }        # pre-condition
    - ramp: { device: psu, channel: voltage, to: 24, over: 30s }
    - loop:
        count: 1000
        steps:
          - ramp:  { device: chamber, channel: temp, to: 85,  over: 10m }
          - dwell: 30m
          - ramp:  { device: chamber, channel: temp, to: -40, over: 10m }
          - dwell: 30m
    - ramp: { device: psu, channel: voltage, to: 0, over: 10s }  # safe shutdown

  guards:                        # always-on abort conditions during the run
    - abort_if: "chamber.temp > 95"
    - abort_if: "rail.current > 5"

  recording:
    channels: [chamber.temp, rail.voltage, rail.current]
    rate: 1 Hz                     # capture config vs. full-rate streaming
```

**Building blocks:**

| Block | Meaning |
| --- | --- |
| `set` / `ramp` / `dwell` | Drive a device channel to a value, sweep over time, hold |
| `loop` | Repeat a sub-sequence N times (or until a condition) |
| `parallel` | Run sub-sequences concurrently (e.g. cycle temp while stepping load) |
| `record` / `mark` | Start/stop capture, tag events for later analysis |
| `guards` | Always-on abort/alert conditions (backed by the pipeline engine) |
| `parameters` | Variables/overrides per run (same script, different values) |

**Runtime:** the agent compiles the script into a deterministic step scheduler; steps send
commands through the same drivers as everything else (`set` → `send_command`); `guards` run
as reactive pipelines in parallel watching every sample; results are written to the local
WAL and (if connected) streamed to the cloud as a versioned **test run** — with recordings,
events, and pass/fail status.

**Authoring & deployment (same as pipes):** edge-local via `tests.yaml`, the local UI script
editor, or `POST /api/v1/tests`; or cloud → fleet (author/version in the Console, deploy to
one or many devices, schedule remotely, collect results back). Scripts keep running fully
offline if the cloud drops.

A full compliance run is: `script` (the procedure) + `pipelines` (the guards) + `recording`
(the evidence) — all as versioned artifacts that can be shared via the marketplace.

### 4.7 Sample Management (`daqcore-sample`)

Sample Management gives every physical unit under test a persistent identity and an
immutable, append-only history: what was done to it, what software it runs, and its
calibration state — from arrival to retirement.

- **Lifecycle state machine** — `received → in-test → calibrated → released`, with
  `quarantined` for failures and a terminal, read-only `graveyard` for retired units.
- **Software state** — firmware version + image hash recorded on every flash, so results are
  attributable to the exact software state.
- **Calibration linkage** — a completed calibration run updates the sample's calibration
  state and due date automatically.
- **Provenance** — every test run, flash, and calibration appends an event; the full history
  is retained even after a unit is scrapped.
- **Storage** — edge-local records (offline-capable) replicate to the cloud into a global
  multi-tenant sample registry.

See [`sample-management.md`](sample-management.md) for the full model and examples.

### 4.8 Remote Access (Secure Shell)

Engineers can reach an edge device (and its lab LAN) remotely **without opening any inbound
ports** and without installing a VPN client. The agent's existing outbound tunnel is reused
as the data path:

```
engineer ──ssh──▶ ssh.daqcore.com ──(agent outbound tunnel)──▶ device sshd :22
```

- **Cloud SSH gateway** authenticates the user (SSO/keys) and maps them to tenant + device.
- **Agent** bridges the proxied bytes to the device's local `sshd`.
- Sessions transit the cloud, enabling **audit & session logging** — a strong compliance
  feature. No lab firewall changes; central revocation from the console.

```bash
ssh bench-07@ssh.daqcore.com        # routes to device "bench-07"
ssh -J ssh.daqcore.com user@bench-07
```

A full Layer-3 WireGuard overlay (arbitrary TCP/UDP to the device's LAN) is a later,
optional tier. See [`remote-access.md`](remote-access.md) for details.

---

## 5. Project Directory Structure

```
DAQcore/
├── Cargo.toml                    # Cargo workspace definition
├── proto/
│   └── daqcore.proto             # Shared Protobuf schemas (Telemetry, Command, Control)
├── crates/
│   ├── daqcore-core/             # Shared types: Sample, Channel, Value, Error, Timestamp
│   ├── daqcore-driver/           # Driver traits, SCPI, Modbus, OPC UA, Mock Bench
│   ├── daqcore-wal/              # Disk ring buffer, segmented WAL, recovery cursor
│   ├── daqcore-pipeline/         # Automation pipes: source→transform→condition→action
│   ├── daqcore-script/           # Test scripts: step scheduler, guards, recording
│   ├── daqcore-sample/           # Sample lifecycle, software/calibration state, graveyard
│   ├── daqcore-transport/        # gRPC client/server codegen, Axum local REST/WebSocket
│   └── daqcore-agent/            # Main edge daemon binary, config parser, orchestrator
└── config/
    └── default.toml              # Example industrial test bench configuration
```

---

## 6. Implementation Roadmap

1. **Workspace & Core Types** — `daqcore-core` data models (`Sample`, `SampleBatch`,
   `ChannelType`, `DeviceStatus`, `Command`) and `proto/daqcore.proto` definitions.
2. **Local Write-Ahead Log Buffer** — binary record encoder/decoder, segment rolling,
   cursor persistence, and tests for crash recovery, corrupt-record detection, and bounded
   capacity rotation.
3. **Pluggable Drivers** — `SyntheticMockBench`, `ScpiDriver`, `ModbusDriver`, and
   `OpcUaDriver` client skeleton.
4. **Local REST / WebSocket Server** — Axum endpoints for streaming live metrics and status
   inspection.
5. **gRPC Upstream Streaming Client** — high-throughput client with auto-reconnect, WAL
   backpressure drain, and remote command receiver.
6. **Automation Pipelines** — declarative pipe runtime on the edge (source→transform→
   condition→action), plus cloud authoring, versioning, and fleet deployment.
7. **Test Scripting** — declarative step scheduler (`set`/`ramp`/`dwell`/`loop`/`parallel`),
   guard integration, and recording capture.
8. **Agent Orchestrator CLI** — configuration loader, task scheduler across hardware
   workers, and graceful lifecycle management.
9. **Verification & Benchmarking** — `cargo test` across crates, plus a standalone
   synthetic benchmark demonstrating 50k+ samples/sec buffered to WAL and streamed locally.

---

## 7. Key Decisions

| Decision | Choice | Rationale |
| --- | --- | --- |
| Initial focus | Edge Agent Core first | Zero-leak Rust daemon is the foundation; cloud builds on top |
| Protocols | SCPI/VISA, Modbus TCP/RTU, OPC UA, Synthetic Mock Bench | Covers lab instruments, PLCs, and offline testing |
| Edge-to-cloud transport | Fast binary streaming (gRPC / QUIC + Protobuf) | Low-latency, strongly typed, high-throughput |
| Backend & dashboard stack | Rust backend + Next.js | Unified Rust edge/cloud, modern web dashboard |

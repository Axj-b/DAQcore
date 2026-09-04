# Extending DAQcore — Drivers, Plugins & Custom Devices

How the Edge Agent runs on hardware, and how to add your own devices, interfaces, and
pipeline stages.

---

## 1. Deployment — Windows, Raspberry Pi, Industrial PC

The Edge Agent is a **single static binary** (`daqcore-agent`) and is fully cross-platform.
It runs natively on:

| Platform | Arch | Packaging |
| --- | --- | --- |
| Windows | x86_64 | `.exe` / `.msi`, runs as a Windows service |
| Linux (Raspberry Pi) | arm64 | `.deb`, runs as a `systemd` service |
| Linux (industrial PC) | x86_64 | `.deb` / binary, runs as a `systemd` service |
| Docker / Podman | amd64 / arm64 | OCI image |
| macOS | arm64 / x86_64 | dev tooling |

No runtime dependencies. The same drivers, WAL, pipelines, scripts, and sample management
work everywhere; only packaging and device-path naming differ (`COM3` on Windows vs
`/dev/ttyUSB0` on Linux).

```bash
# Linux (Raspberry Pi / PC)
curl -sSL https://daqcore.com/install | sh        # or apt install / docker pull
daqcore-agent --config daqcore.toml               # run in foreground
systemctl enable daqcore-agent                    # or run as a systemd service

# Windows (PowerShell)
daqcore-agent.exe --config daqcore.toml
sc.exe create daqcore-agent binPath= "C:\daqcore\daqcore-agent.exe --config C:\daqcore\daqcore.toml"
```

### Docker

The agent ships as a multi-arch OCI image (`amd64` / `arm64`); the self-hostable platform
services (ingest, storage, API, console) ship as a `docker compose` stack.

```bash
# Edge agent in a container
docker run -d --name daqcore-agent \
  --network host \                    # simplifies device/PLC reachability on Linux
  -v ./config:/etc/daqcore:ro \
  -v daqcore-wal:/var/lib/daqcore/wal \
  ghcr.io/daqcore/agent:latest --config /etc/daqcore/daqcore.toml

# Full self-hosted platform (ingest + storage + console)
git clone https://github.com/daqcore/deploy && cd deploy
docker compose up -d
```

Notes: use `--network host` (Linux) when the agent must reach instruments/PLCs on the local
LAN; on Windows/macOS use explicit port mappings and the host gateway for TCP devices.

All behavior comes from a **config file** — devices, channels, sampling rates, pipelines,
endpoints:

```toml
# daqcore.toml
[agent]
name = "bench-07"

[[device]]
id   = "chamber"
type = "scpi"                 # keysight power supply via TCP
host = "192.168.1.42"
port = 5025

[[device]]
id   = "plc"
type = "modbus_tcp"
host = "192.168.1.10"

[storage]
wal_dir = "/var/lib/daqcore/wal"

[upstream]
cloud_url = "grpcs://cloud.daqcore.com"   # optional; omit for fully offline
```

Once running, interact via three surfaces:

- **Local dashboard** — `http://<device-ip>:8080` (live telemetry, pipe/script editor, status)
- **Local REST / CLI** — `daqcore-cli status`, `POST /api/v1/...`
- **Cloud Console** — if configured, the device appears as a managed fleet device.

---

## 2. Config-only vs. Plugin

Rule of thumb: if a device speaks **SCPI, Modbus, OPC UA, or plain serial**, configure it.
If it uses a **vendor-specific or fieldbus protocol** (EtherCAT, CANopen, a robot's
proprietary socket, HID input, camera decode), write a `Driver` implementation — a plugin.

| Device speaks… | Action |
| --- | --- |
| SCPI / VISA | built-in `scpi` driver + config |
| Modbus TCP / RTU | built-in `modbus_tcp` / `modbus_rtu` driver + config |
| OPC UA | built-in `opcua` driver + config |
| Plain serial (line/ASCII protocol) | built-in `serial` driver + config |
| Proprietary / fieldbus / HID / camera | write a custom driver (plugin) |

---

## 3. The Extension Point — the `Driver` trait

```rust
use daqcore_driver::{Driver, Sample, Command, CommandResponse, Result};

pub struct MyThermoDriver { /* port handle, state */ }

#[async_trait]
impl Driver for MyThermoDriver {
    async fn connect(&mut self) -> Result<()> { /* open port / handshake */ }
    async fn sample(&mut self) -> Result<Vec<Sample>> { /* read one cycle */ }
    async fn send_command(&mut self, cmd: Command) -> Result<CommandResponse> { /* ... */ }
    fn metadata(&self) -> DriverMetadata { /* id, channels, units, rates */ }
}
```

Register it so config, scripts, and pipelines can address it by name:

```rust
pub fn register(reg: &mut DriverRegistry) {
    reg.register("my_thermo", MyThermoDriver::builder());
}
```

```toml
[[device]]
id   = "tchamber"
type = "my_thermo"
port = "/dev/ttyUSB0"
```

### Richer device capabilities

Motion and robotics need more than `send_command`. Plugins implement trait supersets so
scripts get typed operations instead of raw strings:

```rust
#[async_trait]
impl MotionAxis for LinearMotor {
    async fn move_abs(&mut self, pos: f64) -> Result<()>;
    async fn move_rel(&mut self, delta: f64) -> Result<()>;
    async fn home(&mut self) -> Result<()>;
    async fn position(&mut self) -> Result<f64>;
    async fn stop(&mut self) -> Result<()>;               // estop
}

#[async_trait]
impl RobotArm for Arm {
    async fn pose(&mut self) -> Result<Pose>;
    async fn move_pose(&mut self, p: Pose) -> Result<()>; // inverse kinematics in driver
    async fn gripper(&mut self, open: bool) -> Result<()>;
}
```

---

## 4. Packaging — two options, same trait

- **Compile in** — add your crate to the Cargo workspace; ship a custom agent binary.
- **Dynamic plugin** — build a shared library and load at startup:

```bash
daqcore-agent --plugin ./my_thermo.so
```

---

## 5. Extending the pipeline & script engine

Custom `transform` and `action` stages implement the same small pattern and register
against the pipe runtime; custom script steps (e.g. a vendor `move_pose`) register against
the step scheduler. Drivers, pipeline stages, and script steps can all be shipped together
as a versioned **bundle** for the marketplace.

# Worked Example — Radar Test Station

A complete example of instrumenting a real radar test station with DAQcore: which devices
are config-only, which need plugins, and how the whole station is scripted into one test.

---

## 1. Station inventory

| Device | Typical interface | DAQcore treatment |
| --- | --- | --- |
| Frequency generator (e.g. Keysight / R&S) | SCPI over LAN/USB | Built-in `scpi` driver + config |
| Frequency analyzer (spectrum analyzer) | SCPI over LAN | Built-in `scpi` driver + config |
| Linear motor X (servo drive) | EtherCAT / CANopen / vendor ASCII | Custom plugin (`linmot`) |
| Linear motor Y (servo drive) | EtherCAT / CANopen / vendor ASCII | Custom plugin (`linmot`) |
| Robot arm (UR / Denso / Dobot) | Proprietary TCP / serial | Custom plugin (`ur_robot`) |
| Barcode / QR reader (DUT ID) | RS-232 / USB HID / camera | Built-in `serial` **or** plugin (`dut_id`) |

Summary: **2 config-only RF instruments + 3–4 plugins (2 motors, 1 arm, 1 reader)**,
packaged together as a single "radar test station" bundle.

---

## 2. RF instruments — no code

```toml
[[device]]
id   = "siggen"
type = "scpi"
host = "192.168.1.50"
port = 5025

[[device]]
id   = "analyzer"
type = "scpi"
host = "192.168.1.51"
port = 5025
```

Driven by SCPI commands from a script:

```yaml
- set:    { device: siggen, command: ":FREQ 77GHz", read: false }
- set:    { device: siggen, command: ":POW -10dBm", read: false }
- record: { device: analyzer, command: ":TRAC:DATA? TRACE1" }
```

---

## 3. Motion plugins — linear motors

Each linear motor implements the `MotionAxis` trait so scripts get typed moves:

```rust
// crate: daqcore-radar-station (plugin bundle)
#[async_trait]
impl MotionAxis for LinearMotor {
    async fn move_abs(&mut self, pos: f64) -> Result<()>;   // mm
    async fn move_rel(&mut self, delta: f64) -> Result<()>;
    async fn home(&mut self) -> Result<()>;
    async fn position(&mut self) -> Result<f64>;
    async fn stop(&mut self) -> Result<()>;                 // estop
}

pub fn register(reg: &mut DriverRegistry) {
    reg.register("linmot", LinearMotor::builder());
}
```

```toml
[[device]]
id    = "motor_x"
type  = "linmot"
bus   = "/dev/can0"
axis  = 1

[[device]]
id    = "motor_y"
type  = "linmot"
bus   = "/dev/can0"
axis  = 2
```

---

## 4. Robot arm plugin

```rust
#[async_trait]
impl RobotArm for Arm {
    async fn pose(&mut self) -> Result<Pose>;
    async fn move_pose(&mut self, p: Pose) -> Result<()>;   // IK handled in driver
    async fn gripper(&mut self, open: bool) -> Result<()>;
}

pub fn register(reg: &mut DriverRegistry) {
    reg.register("ur_robot", Arm::builder());
}
```

```toml
[[device]]
id    = "arm"
type  = "ur_robot"
host  = "192.168.1.60"
```

---

## 5. Barcode / QR reader — DUT identity

Three common reader types map to different drivers:

| Reader | Interface | Treatment |
| --- | --- | --- |
| Industrial RS-232 reader | serial, emits code as a line | built-in `serial` driver + config |
| USB HID "keyboard-wedge" | types the code as keystrokes | plugin reading HID/evdev input |
| Camera-based QR decode | image frames | plugin decoding with a library (`zxing-cpp`) |

Serial reader (config-only):

```toml
[[device]]
id   = "reader"
type = "serial"
port = "/dev/ttyUSB1"
baud = 9600
```

Camera/HID reader (plugin):

```rust
#[async_trait]
impl Driver for BarcodeReader {
    async fn sample(&mut self) -> Result<Vec<Sample>> {
        let code = self.reader.read_line().await?;   // e.g. "SN-2026-04871"
        Ok(vec![Sample::string("dut.serial", code)])
    }
}
```

The scan feeds the test flow — block until scanned, attach the serial to the run, and pick
a per-product recipe:

```yaml
- wait_for:     { device: reader, channel: dut.serial }
- set:          { run_metadata, serial: "${dut.serial}" }
- choose_recipe: { by: "${dut.serial}", prefix: "SN-" }
```

This links into **Device/Asset Management**: every scan creates a record of "this unit was
tested on this station at this time with these results."

---

## 6. Full station config (`daqcore.toml`)

```toml
[agent]
name = "radar-station-01"

[[device]]
id   = "siggen"
type = "scpi"
host = "192.168.1.50"
port = 5025

[[device]]
id   = "analyzer"
type = "scpi"
host = "192.168.1.51"
port = 5025

[[device]]
id    = "motor_x"
type  = "linmot"
bus   = "/dev/can0"
axis  = 1

[[device]]
id    = "motor_y"
type  = "linmot"
bus   = "/dev/can0"
axis  = 2

[[device]]
id    = "arm"
type  = "ur_robot"
host  = "192.168.1.60"

[[device]]
id   = "reader"
type = "serial"
port = "/dev/ttyUSB1"
baud = 9600

[storage]
wal_dir = "/var/lib/daqcore/wal"
```

Loaded at startup with the plugins:

```bash
daqcore-agent --config daqcore.toml \
  --plugin ./linmot.so --plugin ./ur_robot.so --plugin ./dut_id.so
```

---

## 7. Full test script — radar sweep with DUT gating

```yaml
test:
  name: radar-front-end-sweep
  version: 2
  parameters:
    f_start: 76.0
    f_stop:  81.0
    f_step:  0.1
    power:  -10

  steps:
    # 1. Identify the device under test
    - wait_for:     { device: reader, channel: dut.serial }
    - set:          { run_metadata, serial: "${dut.serial}" }
    - choose_recipe: { by: "${dut.serial}", prefix: "SN-" }

    # 2. Position the DUT with the robot arm
    - move_pose: { device: arm, pose: { x: 200, y: 0, z: 150, rz: 90 } }

    # 3. Drive the generator and sweep the analyzer
    - set: { device: siggen, command: ":POW ${power}dBm", read: false }
    - loop:
        var: f
        from: 76.0
        to: 81.0
        step: 0.1
        steps:
          - set:    { device: siggen, command: ":FREQ ${f}GHz", read: false }
          - record: { device: analyzer, command: ":TRAC:DATA? TRACE1", tag: "f=${f}" }

  guards:
    - abort_if: "motor_x.current > 8"
    - abort_if: "arm.estop == true"

  recording:
    channels: [analyzer.trace1, motor_x.pos, motor_y.pos, arm.pose]
    rate: 10 Hz
```

A full compliance run is the combination of:

- **script** — the procedure above (steps + guards + recording)
- **pipelines** — always-on safety reactions (`arm.estop == true → stop all axes`)
- **recording** — the captured evidence tied to the DUT serial

All of it is versioned artifacts, deployable to a fleet from the cloud, and shareable via
the marketplace.

# Worked Example — Calibration Station

A complete example of building a metrology/calibration station with DAQcore: reference
standards, the device under calibration (DUT), the calibration procedure, and certificate
generation — all as config + plugins + a versioned script.

---

## 1. Station inventory

| Device | Typical interface | DAQcore treatment |
| --- | --- | --- |
| Reference calibrator (e.g. Fluke 5730A) | SCPI over LAN/GPIB | Built-in `scpi` driver + config |
| Reference precision DMM (e.g. Keysight 3458A) | SCPI over LAN/GPIB | Built-in `scpi` driver + config |
| Device under calibration (DUT) | SCPI / vendor-specific | `scpi` driver + config, or custom plugin |
| Barcode / QR reader (DUT ID) | RS-232 / USB HID | Built-in `serial` or `dut_id` plugin |
| Environmental probe (temp/humidity) | Modbus RTU / serial | Built-in `modbus_rtu` / `serial` + config |

Summary: **most instruments are config-only** (SCPI/Modbus); only an unusual DUT interface
would need a plugin.

---

## 2. Reference instruments — no code

```toml
[[device]]
id   = "calibrator"
type = "scpi"
host = "192.168.1.70"
port = 5025

[[device]]
id   = "ref_dmm"
type = "scpi"
host = "192.168.1.71"
port = 5025

[[device]]
id   = "dut"
type = "scpi"
host = "192.168.1.72"
port = 5025

[[device]]
id   = "reader"
type = "serial"
port = "/dev/ttyUSB1"
baud = 9600

[[device]]
id   = "env"
type = "modbus_rtu"
port = "/dev/ttyUSB0"
baud = 19200
```

---

## 3. Traceability model

Calibration adds one concept the generic platform doesn't have: **traceability** — a chain
from the DUT, through the working standards, to the national standard. This is expressed as
data attached to the station (not code):

```toml
[traceability]
chain = [
  { asset = "calibrator", cert = "CAL-2026-011", std = "NIST 5730A" },
  { asset = "ref_dmm",    cert = "CAL-2026-009", std = "NIST 3458A" },
]
```

Every run records which standard was used, so certificates can state "traceable to … via
cert …".

---

## 4. Calibration script — as-found / adjust / as-left

```yaml
test:
  name: dc-voltage-calibration
  version: 4
  parameters:
    points: [0, 1, 5, 10, 20, 30]
    tolerance: 0.05        # ±0.05 %
  recording:
    channels: [dut.voltage, ref_dmm.voltage, env.temp, env.humidity]
    rate: 1 Hz

  steps:
    # 1. Identify the DUT
    - wait_for: { device: reader, channel: dut.serial }
    - set:      { run_metadata, serial: "${dut.serial}" }
    - mark:     { label: "env", temp: "${env.temp}", humidity: "${env.humidity}" }

    # 2. As-found measurement (before any adjustment)
    - mark: { label: "phase", value: "as-found" }
    - loop:
        var: v
        values: [0, 1, 5, 10, 20, 30]
        steps:
          - set:    { device: calibrator, command: ":OUT ${v}V", read: false }
          - dwell:  2s
          - record: { device: ref_dmm, channel: "MEAS:VOLT:DC?", tag: "ref=${v}" }
          - record: { device: dut,    channel: "MEAS:VOLT:DC?", tag: "dut=${v}" }

    # 3. Adjust (only if out of tolerance) — example of a conditional adjustment
    - adjust:
        if: "error_percent > tolerance"
        device: dut
        procedure: auto-cal        # vendor-defined adjustment entry point

    # 4. As-left measurement (after adjustment)
    - mark: { label: "phase", value: "as-left" }
    - loop:
        var: v
        values: [0, 1, 5, 10, 20, 30]
        steps:
          - set:    { device: calibrator, command: ":OUT ${v}V", read: false }
          - dwell:  2s
          - record: { device: ref_dmm, channel: "MEAS:VOLT:DC?", tag: "ref=${v}" }
          - record: { device: dut,    channel: "MEAS:VOLT:DC?", tag: "dut=${v}" }

    # 5. Emit certificate
    - report: { template: "calibration-certificate", format: "pdf", sign: true }

  guards:
    - abort_if: "env.humidity > 60"
    - abort_if: "ref_dmm.error == true"
```

The `report` step pulls the traceability chain, the recorded as-found/as-left points, the
tolerance result, and the DUT serial into a versioned certificate — exactly what a
compliance lab signs off.

---

## 5. Pipelines — always-on lab guards

Alongside the script, reactive pipelines keep the station safe and audit-ready:

```yaml
pipelines:
  - id: env-guard
    source: { channels: ["env.humidity", "env.temp"] }
    condition:
      expr: "env.humidity > 60"
    action:
      - type: alert
        severity: warning
        message: "Lab humidity outside calibration limits"

  - id: standard-due-guard
    source: { channels: ["calibrator.state"] }
    condition:
      expr: "calibrator.next_cal_due < now + 30d"
    action:
      - type: alert
        severity: critical
        message: "Standard calibration due soon"
```

---

## 6. What a metrology lab gets

- **Config-only instruments** for calibrator, DMM, and DUT (SCPI).
- **DUT identity** via barcode, linked to every run and certificate.
- **Versioned procedure** (as-found → adjust → as-left) that runs offline on the edge.
- **Traceability chain** recorded per run, surfaced in the certificate.
- **Asset history** — every unit, every standard, every due date tracked over time.

The same platform, scripts, and pipelines that drive a radar test station therefore also
drive a calibration station — only the recipes (scripts, report templates, traceability
metadata) differ.

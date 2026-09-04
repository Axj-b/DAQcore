# Sample Management — Tracking Every Unit Under Test

Sample Management gives every physical unit under test (a "sample") a persistent identity
and a full, auditable history: what was done to it, what software it runs, and its
calibration state — from arrival to retirement.

It solves the recurring problem: *"which exact unit was this, what firmware did it have,
was it calibrated, and what happened to it?"*

---

## 1. What is a sample?

A **sample** is a tracked physical object — a device under test (DUT), a unit, a board, a
module. It is identified by a serial number (from a barcode/QR scan) and carries a
structured record that accumulates over its lifetime.

```yaml
sample:
  id: "SN-2026-04871"
  product: "radar-front-end"
  location: "radar-station-01"       # current station / shelf
  lifecycle:
    state: in-test                   # see state machine below
    since: 2026-09-01T09:12:00Z
  software:
    firmware: "v2.3.1"
    image_hash: "sha256:3f9a…"
    flashed_at: 2026-09-01T08:40:00Z
  calibration:
    state: calibrated                # uncalibrated / calibrated / expired
    last_run: "CAL-2026-04871"
    due: 2027-09-01
  history:                           # append-only, immutable
    - { event: received,        at: 2026-08-28T10:00:00Z }
    - { event: flashed,         firmware: "v2.3.1", at: 2026-09-01T08:40:00Z }
    - { event: test_run,        test: radar-front-end-sweep, result: pass, at: 2026-09-01T12:30:00Z }
    - { event: calibrated,      run: CAL-2026-04871, result: pass, at: 2026-09-02T09:00:00Z }
```

---

## 2. Lifecycle state machine

Every sample moves through explicit, recorded states. Transitions are events in the history.

```
received ──▶ in-test ──▶ calibrated ──▶ released
    │            │              │
    └──────▶ quarantined ◀──────┘
                  │
                  ▼
              graveyard          (terminal, read-only)
```

| State | Meaning |
| --- | --- |
| `received` | Arrived, logged, not yet processed |
| `in-test` | Actively being tested / flashed |
| `calibrated` | Passed calibration, valid until `due` |
| `released` | Passed and shipped / returned to service |
| `quarantined` | Failed or suspect — blocked from further use |
| `graveyard` | Retired / disposed — history retained forever |

Rules (enforced by the platform):

- A `quarantined` sample cannot enter `in-test` or `calibrated` without a `released`/re-open
  transition.
- The `graveyard` is terminal: nothing can move out, but nothing is ever deleted.

---

## 3. Software state tracking

Every flash/update is recorded against the sample:

```yaml
- event: flashed
  firmware: "v2.3.1"
  image_hash: "sha256:3f9a…"
  by: "ci/build-4821"
  at: 2026-09-01T08:40:00Z
```

Because `software.firmware` and `image_hash` are part of the sample record, a test result is
always attributable to the exact software state — no more "we think it ran v2.3.1".

---

## 4. Calibration linkage

Calibration is not a separate island — completing a calibration run **updates the sample
automatically**:

```
calibration run "CAL-2026-04871" (pass)
   └─▶ sample.calibration.state = calibrated
   └─▶ sample.calibration.last_run = CAL-2026-04871
   └─▶ sample.calibration.due = <next interval>
   └─▶ history += { event: calibrated, run: CAL-2026-04871, result: pass }
```

The `calibrated`/`expired` state is derived from the last run and the interval, so a
`calibration.due` guard can flag samples before they lapse (see §6).

---

## 5. The graveyard

The **graveyard** is the terminal archive for samples that are retired, scrapped, or
otherwise finished — kept read-only for traceability.

- Sample moves to `graveyard` with a reason (`scrapped`, `EOL`, `lost`, `returned-to-vendor`).
- History, software state, calibration records, and all test results remain queryable.
- Purpose: full audit trail for compliance ("what happened to unit X over its whole life?")
  without polluting active inventory.

```yaml
- event: retired
  to: graveyard
  reason: "scrapped"
  at: 2029-03-01T00:00:00Z
```

---

## 6. Integration points

| Source | Effect on the sample |
| --- | --- |
| Barcode/QR scan | Creates or resolves the sample (`dut.serial`) |
| Test script (`set run_metadata`) | Attaches the sample to the run; appends `test_run` event + result |
| Flash step | Records `firmware` + `image_hash` |
| Calibration run | Sets calibration state + due date (see §4) |
| Pipeline guard | e.g. `calibration.due < now + 30d → alert`, or block test on `quarantined` |

Example guard that blocks work on an uncalibrated sample:

```yaml
pipelines:
  - id: calibration-gate
    source: { channels: ["sample.calibration.state"] }
    condition:
      expr: "sample.calibration.state != 'calibrated'"
    action:
      - type: block_test
        message: "Sample not calibrated"
      - type: alert
        severity: critical
        message: "Blocked: sample ${sample.id} not calibrated"
```

---

## 7. Where it lives — edge & cloud

- **Edge-local:** each agent holds the sample records for units it touches, so a station
  works fully offline. The scan → sample resolution and local history writes happen on the
  device.
- **Cloud-synced:** sample records replicate to the platform, giving a global, multi-tenant
  registry — "find every unit ever tested, everywhere."
- **Asset/standards:** reference standards and instruments are samples too, so their own
  calibration/traceability chain is tracked by the same mechanism.

---

## 8. What this unlocks

- **Full provenance** — every unit carries its complete, immutable history.
- **Software-state attribution** — results tied to exact firmware + image hash.
- **Automatic calibration state** — linked directly after each calibration run.
- **A graveyard** — retired units retained read-only for audit and compliance.
- **Gates** — tests and calibrations blocked on invalid states (quarantined, expired).

This is one of the platform's core "data domains" — the master record for physical units —
alongside channel/measurement configuration and time-series/test artifacts.

# HIL Testing — Power-Loss & Crash Recovery Rig

A local hardware-in-the-loop (HIL) rig for validating DAQcore's crash-safety claims on real
target hardware: kill power to the agent mid-acquisition, then verify the recovered WAL is
CRC-clean, continuous up to the last fsync, and contains a *bounded, detectable* gap — never
garbage.

It is the physical complement to the unit-level crash-recovery tests in
[`roadmap.md`](roadmap.md) step 2. Instead of `kill -9`, it cuts real mains power via a
Rohde & Schwarz supply and replays Arduinos with predefined curves as independent ground truth.

---

## 1. What this validates

| Claim | How the rig proves it |
| --- | --- |
| No silent corruption | CRC32 per WAL record — a corrupted record fails decode instead of passing |
| Crash-safe recovery | WAL reopens to a consistent state after power returns |
| Bounded data loss | Gap in `seq` is measurable and tunable via `durability` |
| Zero memory leaks | Soak the DUT Pi for hours/days, watch RSS stay flat |
| Segment rotation under load | WAL rolls segments correctly across many cycles |

---

## 2. What it can and cannot prove

**Cannot prove "zero loss."** Samples sitting in RAM (not yet fsynced) are inevitably gone
when power dies — that is physics, not a bug. The `durability` setting trades throughput
against how much can be lost:

| `durability` | fsync behavior | Loss window on power cut |
| --- | --- | --- |
| `sync_all` | fsync every append | smallest (near-zero) |
| `sync_data` | fsync data (default) | small |
| `interval` | fsync every `fsync_interval_ms` | up to that interval |
| `none` | rely on OS | largest |

**The real assertion:** after power returns, the recovered WAL is continuous up to the last
fsynced `seq`, followed by a *gap* (not corruption). "Zero loss" is only achievable by
paying for `sync_all`.

---

## 3. Rig layout

```
┌─ Control Pi (test harness) ─────────────────────────────┐
│  - commands the R&S PSU over SCPI/LAN                   │
│  - runs the cut cycle: wait → cut → wait → restore      │
│  - pulls the DUT WAL, runs verification, writes a report │
└──────────────────────────┬──────────────────────────────┘
                           │ SCPI (LAN / USBTMC)
                ┌──────────▼──────────┐
                │  R&S power supply   │── mains on/off ──► [ DUT Pi: daqcore-agent ]
                └─────────────────────┘                    ▲
                                                           │ serial / Modbus
                                   ┌───────────────────────┴───────────────────────┐
                                   │ Arduino sensors: predefined curves + seq        │
                                   │ (powered by a SEPARATE supply so they survive) │
                                   └────────────────────────────────────────────────┘
```

Two details that matter:

1. **Power the Arduinos separately** (not through the R&S). If the sensor dies with the Pi,
   you cannot tell "sensor crashed" from "Pi crashed", and you lose your reference.
2. The **R&S PSU is itself SCPI** — the same protocol the built-in `scpi` driver speaks. The
   harness can drive it as a DAQcore driver, which doubles as a test of that path.

---

## 4. Components

| Component | Role | Notes |
| --- | --- | --- |
| Control Pi | Orchestrates the cut cycle + verification | One of the spare Pis |
| R&S power supply | Cuts/restores mains to the DUT | SCPI over LAN; any R&S `NGL`/`NGM`/`HMP` series |
| DUT Pi | Runs `daqcore-agent` under test | Any Pi; logs RSS/CPU to a *separate* store |
| Arduino sensors | Emit predefined curves with per-sample `seq` | Powered independently; serial or Modbus RTU |
| Reference log | Expected values for every sample | Reconstructed from the predefined curve + `seq` |

---

## 5. Ground truth — independent source of error detection

To detect an error you need a reference that survives the cut **independently of the DUT**:

1. **Predefined curves** — sine/ramp/step (the mock bench already emits `thermal`,
   `dc_sweep`, `noise`; an Arduino emits the same shapes over real serial). Expected values
   are reconstructable offline.
2. **Monotonic sequence numbers** — stamp a strictly increasing counter per channel *at the
   source*, not the Pi's clock. Any gap becomes instantly obvious after recovery.

Never rely on the DUT's wall-clock `ts` to detect loss — that clock is what just lost power.
`SampleBatch.seq` is the authoritative cursor; the Arduino's per-sample counter is the
independent cross-check.

---

## 6. Test cycle (per run)

```
1. start Arduino emitters (known curve + seq)
2. start daqcore-agent on the DUT, durability = <mode under test>
3. wait N seconds (steady-state acquisition)
4. command R&S PSU: output OFF (cut power to DUT)
5. wait M seconds (power fully gone)
6. command R&S PSU: output ON
7. wait for the DUT to reboot + agent to start
8. pull the DUT WAL to the control Pi
9. verify: CRC-clean, seq continuity, gap size, curve diff vs reference
10. append result to the run report
```

Vary the cut **at random intervals** — the nastiest bugs are power dying mid-write or
mid-segment-roll, and only random timing catches those. Run each durability mode through the
same cycle and record the measured loss window per mode.

---

## 7. Pass criteria

| Metric | Pass criterion |
| --- | --- |
| Recovered WAL is CRC-clean | zero corrupt records on decode |
| Seq continuity | no gap *interior* to the recovered data |
| Max gap at tail | ≤ the expected window for the `durability` mode |
| Curve diff vs reference | reconstructed values match the predefined curve |
| RSS over soak | flat (no monotonic growth) |
| Segment rotation | segments roll correctly across cycles |

---

## 8. Build phases

1. **Phase 1 — cheapest, works today.** Mock bench + R&S power cut on one DUT Pi. No
   Arduinos: the mock bench already emits deterministic curves. Validates WAL recovery end to
   end.
2. **Phase 2 — real ingest.** Add Arduinos over serial/Modbus to exercise the driver ingest
   path and get an external observer that survives the cut.
3. **Phase 3 — fleet.** Multiple DUT Pis, coordinated cuts, cross-compare results; fold in
   the master/slave topology from [`architecture.md`](architecture.md) §4.9.

---

## 9. Example DUT config

```toml
# daqcore-agent --config hil-dut.toml
[agent]
name = "hil-dut-01"

[storage]
wal_dir = "/var/lib/daqcore/wal"
durability = "sync_data"      # vary this per run: sync_all | sync_data | interval | none
fsync_interval_ms = 1000

[sampling]
interval_ms = 100
batch_size = 100

[[device]]
id   = "chamber"
type = "mock"

[[device.channels]]
id = "chamber.temp"
unit = "degC"

[device.channels.signal]
kind = "thermal"
ambient = 40.0
amplitude = 20.0
period_secs = 60.0
```

The harness (control Pi) drives the R&S supply over SCPI and runs the cycle; in later phases
it is itself a `daqcore-agent` in `master` mode driving the DUT as a slave.

---

## 10. What this unlocks

- **Confidence in the durability guarantee** — real, physical evidence that the WAL does what
  the design claims, not just unit-test coverage.
- **A repeatable regression harness** — every new agent version runs the same cut cycle to
  catch recovery regressions before they ship.
- **Early validation of the deploy path** — cross-compiling to ARM (`aarch64`) and running on
  the Pi becomes a first-class, exercised workflow while the codebase is still small.

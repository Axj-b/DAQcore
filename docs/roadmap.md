# Feature Backlog & Roadmap

The complete feature backlog for the DAQcore platform, prioritized into tiers. **P0** items
are the ones that must be implemented first — they make the platform a complete,
professional product.

---

## Priority legend

| Tier | Meaning |
| --- | --- |
| **P0** | Must implement first — product completeness |
| P1 | Near-term — right after P0 |
| P2 | Later — platform maturity |
| P3 | Ecosystem / business scale |

---

## P0 — Must implement first

These turn the core into a ready-to-use, professional product.

| # | Feature | Why it's first |
| --- | --- | --- |
| 1 | **Standard export formats** — TDMS, HDF5, CSV, MAT | Labs live in MATLAB/LabVIEW; without export there is no adoption |
| 2 | **Notifications** — email / Slack / Teams / webhook | Alerts only in a dashboard get missed; guards must reach people |
| 3 | **Fleet OTA & agent self-update** — version the agent + device config | You already deploy pipes; the binary and config need the same lifecycle |
| 4 | **Approval workflows & audit-grade controls** — 21 CFR Part 11, e-signatures, immutable audit trail | Converts "nice to have" into "required" for regulated labs; premium tier |
| 5 | **Python SDK + Jupyter** | Engineers analyze runs with pandas, not just dashboards |
| 6 | **Long-term storage tiering** — hot → warm → cold (S3/object) | Durability tests run for months; retention without full-res cost |
| 7 | **EU Cyber Resilience Act (CRA) compliance** — security-by-design, SBOM, vulnerability handling | Mandatory to sell software in the EU (~2027); see `compliance.md` |

---

## P1 — Near-term

| # | Feature |
| --- | --- |
| 7 | Anomaly detection / predictive maintenance (statistical first) |
| 8 | Golden-baseline comparison — auto-diff runs against a known-good reference |
| 9 | Annotations & comments on runs and timestamps |
| 10 | Grafana / Prometheus export |
| 11 | Device identity & mTLS — per-device certificates, signed firmware, encrypted WAL at rest |
| 12 | Secrets management — credentials never in plain config |
| 13 | SSO / SCIM + per-tenant data isolation (OIDC / SAML) |
| 14 | Metering & billing service — foundation for usage-based cloud pricing |

---

## P2 — Later

| # | Feature |
| --- | --- |
| 15 | MES / ERP / PLM connectors — push results and calibration state outward |
| 16 | More protocols — CAN/CANopen, EtherCAT, Profinet, EtherNet/IP, LXI, GPIB/VISA-USB, BACnet |
| 17 | Native DAQ drivers — National Instruments, Keysight DAQ970A, Dewesoft |
| 18 | Vision / camera — optical inspection during a test cycle |
| 19 | Data import standards — ECLASS / eCl@ss / GS1 classification (feeds Sample Management) |
| 20 | AI-assisted authoring — generate pipes/scripts and summarize reports |

---

## P3 — Ecosystem / business scale

| # | Feature |
| --- | --- |
| 21 | Template / recipe library — community-shared pipes, scripts, report packs |
| 22 | ML-driven anomaly detection and fleet health scoring |

---

## Suggested build order (shortlist)

1. Export formats (TDMS / HDF5 / CSV)
2. Notifications
3. Fleet OTA & agent self-update
4. Approval workflows + audit-grade controls
5. Python SDK
6. Long-term storage tiering
7. CRA compliance (security-by-design, SBOM, vulnerability handling)

…then proceed through P1 → P2 → P3 as adoption and revenue allow.

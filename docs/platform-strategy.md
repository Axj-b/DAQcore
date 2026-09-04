# DAQcore Platform Strategy — Open-Core Industrial Data Platform

> DAQcore is a modular open-core platform for the industrial test & measurement domain: a
> single system that turns raw sensor data from test benches, hardware compliance labs, and
> durability tests into actionable engineering insight.

---

## 1. Positioning — the "Test Data Spine"

DAQcore is the unifying data layer for the lab floor: one platform spanning test-bench
device control, measurement data, and durability/compliance insight — from raw sensor →
local Edge Agent → cloud telemetry → engineering dashboards and compliance reports.

| Platform layer | DAQcore component |
| --- | --- |
| Test Data Spine | Raw signal → actionable insight |
| Core framework + Bundles | Rust core + pluggable driver/connector bundles |
| Admin UI | DAQcore Console (Next.js dashboard) |
| Marketplace | DAQcore Registry + Storefront |
| Editions | Community (Apache-2.0) / Enterprise / DAQcore Cloud |

---

## 2. License & Editions (Open-Core)

The core is permissive open source; monetization happens in the cloud, enterprise modules,
marketplace, and services layers.

- **Free Edge (Apache-2.0, self-contained):** the Edge Agent + local dashboard run fully
  standalone — no cloud, no account, no license required. Offline buffering, local
  dashboards, and remote control over the local API are free forever. This is the
  foundational "free edge possibility": every test bench can be instrumented at zero cost.
- **Core Platform = Apache-2.0** (permissive): ingest, storage, API, single-node
  self-hosted deployment. Free forever, no copyleft friction, maximum adoption.
- **Enterprise Edition (commercial, self-host):** multi-tenancy, RBAC/SSO/OIDC, audit log,
  retention/policy engine, compliance report packs, advanced analytics, HA clustering.
- **DAQcore Cloud (paid managed PaaS):** hosted multi-tenant SaaS — the primary
  monetization lever, and entirely optional; the edge never depends on it.

---

## 3. Modular Platform Architecture

```
DAQcore/
├── crates/ (Rust, Apache-2.0)
│   ├── daqcore-core        # shared types
│   ├── daqcore-driver      # Driver trait + SCPI/Modbus/OPC UA/Mock
│   ├── daqcore-wal         # crash-safe local buffer
│   ├── daqcore-transport   # gRPC/QUIC + Axum local API
│   └── daqcore-agent       # edge daemon
├── platform/ (self-hostable services)
│   ├── ingest/             # telemetry ingest (gRPC/WS)
│   ├── storage/            # time-series + metadata + artifacts
│   ├── control/            # remote command router
│   ├── auth/               # multi-tenant identity/RBAC
│   └── api/                # unified GraphQL/REST gateway
├── console/                # Next.js dashboard (DAQcore Console)
├── bundles/                # installable extensions (drivers, connectors, reports)
├── deploy/                 # docker-compose + Helm (any cloud / on-prem)
└── marketplace/            # registry + storefront
```

**Bundles** — each is a versioned, independently installable unit: protocol drivers, cloud
connectors, report templates, analytics packs, compliance certifications.

---

## 4. Infrastructure

- **Self-hostable everywhere:** Docker Compose for single-node; Helm chart / Kubernetes for
  scale — runs on AWS/GCP/Azure or bare-metal. No vendor lock-in.
- **DAQcore Cloud:** the same stack operated as a managed multi-tenant PaaS (billing,
  metering, fleet onboarding).
- **Marketplace:** GitHub-backed bundle registry + lightweight storefront (free & paid
  drivers/connectors/reports), license-key service for paid bundles.
- **CI/CD + Docs + Release pipeline:** per-crate and per-bundle release, signed artifacts,
  auto-generated docs site.

---

## 5. Business Model

1. **Free edge + free open core** (Apache-2.0) — every bench can be instrumented at zero
   cost (local dashboards + offline buffering); drives adoption and the developer community.
2. **DAQcore Cloud subscription** — optional; tiered by devices, data retention, throughput
   (primary revenue). The edge never requires it.
3. **Enterprise self-host licenses** — for compliance labs that must run air-gapped/on-prem.
4. **Marketplace revenue share** — paid drivers/connectors/report packs.
5. **Partner program** — system integrators & compliance labs as solution partners.
6. **Services** — certification, custom drivers, compliance validation, migration.

---

## 6. Open Questions (deferred)

- **Product pillars** — the core data domains for DAQcore. Confirmed so far: **Sample &
  Unit Management** (unit lifecycle, software & calibration state, graveyard — see
  [`sample-management.md`](sample-management.md)). Remaining candidates to finalize:
  Channel/Point Configuration, Time-Series & Test Artifacts, Dashboards & Reporting.
- **Cloud specifics** — exact provider, storage engine (TimescaleDB vs ClickHouse), metering
  model.
- **Marketplace depth** — registry + storefront scope and launch timing.

---

## 7. Next Steps

1. Write this strategy alongside `docs/architecture.md` (done).
2. Scaffold the open-core skeleton: Cargo workspace + Apache-2.0 headers + first driver
   (Mock Bench) + WAL.
3. Stand up the self-hostable platform services (ingest, storage, API) with
   docker-compose/Helm.
4. Build the DAQcore Console dashboard.
5. Introduce the bundle registry + storefront and the partner program.

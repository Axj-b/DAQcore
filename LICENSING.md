# DAQcore Licensing Model Overview

DAQcore operates on an **open-core** business model, cleanly splitting the platform
between a freely accessible, open-source edge component and a monetized, proprietary
cloud/enterprise tier.

## The Edge Tier (Open-Source — Apache 2.0)

**Scope:** the core Rust Edge Agent, hardware protocol drivers (SCPI, Modbus, OPC UA),
the Write-Ahead Log (WAL) offline buffer, and the embedded local web dashboard
(`edge/` directory).

**Terms:** licensed under the permissive Apache License 2.0. Anyone can freely download,
use, modify, and commercially deploy the edge software on local hardware (Raspberry Pi,
industrial PCs) without paying royalties or hitting revenue caps. This ensures
frictionless adoption by hardware and compliance engineers.

See [`LICENSE-APACHE`](LICENSE-APACHE).

## The Cloud & Enterprise Tier (Proprietary / Commercial)

**Scope:** the multi-tenant SaaS telemetry ingestion backend, the central cloud dashboard,
fleet-wide orchestration, secure cloud-to-edge SSH tunneling, and regulated compliance
audit packages (e.g., VDE / 21 CFR Part 11) (`cloud/` directory).

**Terms:** proprietary, closed-source software governed by a commercial agreement or SaaS
subscription model. Monetized via per-device monthly cloud subscriptions or self-hosted
enterprise licensing.

See [`LICENSE-COMMERCIAL`](LICENSE-COMMERCIAL).

## Repository Enforcement

The codebase is structured as a monorepo with strict directory separation (`edge/` vs.
`cloud/`), backed by explicit file-level SPDX license identifiers and this documentation
to protect intellectual-property boundaries.

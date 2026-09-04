# Compliance — CRA, GDPR, and Certification

The compliance landscape for DAQcore as a **software platform** (no hardware shipped).
CE marking is intentionally out of scope; the two relevant regimes are the EU Cyber
Resilience Act and the GDPR.

---

## 1. Summary

| Regime | Applies? | Scope |
| --- | --- | --- |
| **CE marking** | No | Only for physical/hardware products placed on the EU market. DAQcore ships software, so CE is not required. (Revisit only if a hardware appliance/gateway is ever sold.) |
| **EU Cyber Resilience Act (CRA)** | Yes — primary | Security requirements for digital products; the main compliance target. |
| **GDPR** | Yes — limited | Personal data in accounts/logs and any personal data customers store. |

---

## 2. EU Cyber Resilience Act (CRA)

The CRA is the core compliance obligation for software like DAQcore. It mandates
security-by-design for products with digital elements, applying to the Edge Agent, the
cloud services, and distributed bundles.

**Obligations:**

- **Security by design & default** — secure development lifecycle, no known exploitable
  vulnerabilities at release.
- **Vulnerability handling** — documented process, security updates, and a single point of
  contact (PSIRT) for reporting.
- **Conformity assessment** — technical documentation + EU declaration of conformity;
  a self-assessment path is available for non-critical software.
- **Transparency** — SBOM (software bill of materials) and clear support/update timelines.
- **Incident reporting** — exploited vulnerabilities reported to ENISA.

**How DAQcore's roadmap already maps to CRA:**

| CRA requirement | DAQcore feature |
| --- | --- |
| Secure-by-design | Rust core (memory safety), signed artifacts |
| Device security | Per-device identity & mTLS, signed firmware |
| Data protection | Encrypted write-ahead log at rest |
| Accountability | Immutable audit trail |
| Access control | SSO / RBAC / per-tenant isolation |
| Secrets | Central secrets management |

CRA is phasing in through ~2027; it should be treated as a **P0** engineering requirement.

---

## 3. GDPR

GDPR applies to **personal data**, not industrial telemetry. Sensor readings, waveforms,
test results, calibration records, and sample serial numbers are generally *not* personal
data, so the GDPR surface area is small.

**In scope:**

- User accounts (names, emails) in the cloud/console.
- Identifiers in logs (IP addresses, usernames, "who ran this test").
- Any personal data a customer stores (operator names, DUTs tied to a person).

**Roles:**

| Role | When | Implication |
| --- | --- | --- |
| Controller | DAQcore's own customer/account data | Lawful basis, privacy policy, data-subject rights |
| Processor | Hosting customer data in DAQcore Cloud | Data Processing Agreement (Art. 28), follow instructions, Art. 32 security |

**Obligations (checklist):**

- Data Processing Agreement (DPA) with each cloud customer.
- EU data residency (or SCCs for any third-country transfer).
- Data minimization (natural for telemetry; don't log more than needed).
- 72-hour breach notification to supervisory authorities.
- Records of processing + privacy policy.
- Art. 32 security — mapped to the same mTLS/encryption/SSO/audit features as CRA.

---

## 4. Certification as a selling point

- **CRA readiness** — frame as the platform's security guarantee for enterprise buyers.
- **21 CFR Part 11** — optional, for customers in regulated (FDA-adjacent) labs.
- **GDPR** — a hygiene item; mostly satisfied by EU hosting + a DPA template.

A compliance summary table (CRA + GDPR + optional Part 11) is a natural addition to the
marketing site and to enterprise sales materials.

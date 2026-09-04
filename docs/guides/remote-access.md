# Remote Access — Secure Shell over the Cloud

How engineers reach an edge device (and its lab LAN) remotely, without opening any inbound
ports and without installing anything on their laptop.

---

## 1. The idea

The Edge Agent already holds a persistent **outbound** connection to the cloud. Remote
access rides that existing tunnel: the cloud exposes an SSH endpoint, authenticates the
user, and proxies the session down the agent's channel to the device's local `sshd`.

```
engineer ──ssh──▶ ssh.daqcore.com ──(agent outbound tunnel)──▶ device sshd :22
```

- **No inbound ports** on the device or the lab firewall.
- **No VPN client** for the engineer — just standard `ssh`.
- Sessions transit the cloud, so they can be **logged and audited**.

---

## 2. Usage

```bash
ssh bench-07@ssh.daqcore.com        # routes to device "bench-07"
# or, hop through the gateway:
ssh -J ssh.daqcore.com user@bench-07
```

The username before the `@` (or the host alias) maps to a device ID in the tenant's fleet;
the cloud resolves it, checks permissions, and opens the tunnel.

---

## 3. Architecture

| Component | Role |
| --- | --- |
| **SSH gateway** (cloud) | Terminates user SSH, authenticates via SSO/keys, maps user → tenant → device |
| **Tunnel channel** | The existing agent → cloud gRPC/QUIC stream, reused as the data path |
| **Agent (edge)** | Opens the outbound connection; on request, bridges bytes to local `sshd` on `127.0.0.1:22` |
| **Local sshd** | The device's own SSH server (unchanged) |

Auth and authorization live in the cloud (same RBAC/SSO as the console), so access is
multi-tenant and revocable from one place.

---

## 4. What you get for free by proxying

- **Audit & session logging** — every SSH session can be recorded and tied to a user and a
  device. This is a strong compliance-lab selling point.
- **Central revocation** — disable a user or a device and access stops immediately.
- **No lab firewall changes** — the agent dials out; nothing is opened inbound.

---

## 5. Scope & limitations

- Carries SSH (and any TCP forwarded over it). For arbitrary protocols to the device's LAN
  (camera streams, PLC web UIs, file shares), a full Layer-3 overlay is the next step
  (see the WireGuard overlay option in
  [`architecture.md`](architecture.md#48-remote-access-secure-shell)).

---

## 6. Fit with the business model

Remote access is a **DAQcore Cloud / Enterprise** feature: the free edge works standalone,
but "reach your bench from anywhere, with audited SSH" is a paid tier — exactly what
compliance labs and distributed engineering teams pay for.

# CDAP Implementation Plan

> **Version**: 1.0.0  
> **Status**: Draft  
> **Created**: 2026-03-19  
> **Depends on**: [CUSTOM_DEVICE_API.md](CUSTOM_DEVICE_API.md) v0.2.0

---

## Table of Contents

1. [Overview](#overview)
2. [Architecture Decisions](#architecture-decisions)
3. [Authentication & 2FA Integration](#authentication--2fa-integration)
4. [RustDesk Client Synchronization](#rustdesk-client-synchronization)
5. [Implementation Phases](#implementation-phases)
6. [File-Level Change Map](#file-level-change-map)
7. [Testing Strategy](#testing-strategy)
8. [Migration & Backward Compatibility](#migration--backward-compatibility)
9. [Risk Assessment](#risk-assessment)

---

## Overview

This document translates the CDAP specification (CUSTOM_DEVICE_API.md) into a concrete implementation plan covering:

- **CDAP Gateway** — WebSocket server on port 21122 in Go server
- **Auth integration** — CDAP clients authenticate via the same user/2FA system as the Node.js panel and RustDesk clients
- **RustDesk synchronization** — CDAP native desktop clients coexist with RustDesk clients in the same peer table, address books, connection history, and panel views
- **Media channel** — Binary frame relay for remote desktop, video, audio, input
- **Native Yomie client** — Desktop agent that combines remote desktop + OS management

### Core Principle

> **One identity, one authority.** A user account created in the Node.js panel works for panel login, RustDesk client login, AND CDAP native client login — same credentials, same 2FA, same RBAC.

---

## Architecture Decisions

### AD-1: Auth Source of Truth

**Decision**: The Go server is the auth source of truth for all protocol-level authentication (RustDesk clients, CDAP clients, API keys). The Node.js panel is the auth source of truth for panel UI sessions (cookie-based).

**Rationale**: RustDesk clients already authenticate via Go server's `/api/login` (port 21114). CDAP clients will use the same endpoint or a CDAP-specific auth message that routes to the same Go auth backend. This avoids auth duplication and ensures 2FA state is consistent.

```
┌──────────────────────────────────────────────────────┐
│                 Authentication Flow                    │
│                                                        │
│  Panel User       → Node.js session (cookie)           │
│    └─ 2FA check   → Node.js auth.db (TOTP)            │
│                                                        │
│  RustDesk Client  → Go server /api/login (JWT)         │
│    └─ 2FA check   → Go server TOTP (5-min partial)    │
│                                                        │
│  CDAP Client      → Go server /ws/cdap auth msg (JWT)  │
│    └─ 2FA check   → Go server TOTP (same path)        │
│                                                        │
│  Panel ↔ Go       → X-API-Key header (.api_key file)  │
│                                                        │
│  ──── Shared ─────────────────────────────────────     │
│  Users table:   Go server db (users)                   │
│  TOTP secrets:  Go server db (users.totp_secret)       │
│  Roles/RBAC:    Go server db (users.role)              │
│  API keys:      Go server db (api_keys)                │
│  Panel sessions: Node.js auth.db (separate, UI only)   │
└──────────────────────────────────────────────────────┘
```

### AD-2: Dual Auth Database Convergence

**Current state**: Node.js panel has `auth.db` (users, sessions) and Go server has `db_v2.sqlite3` (users, api_keys). Both have user tables with potentially different passwords and TOTP states.

**Decision**: Phase 1 keeps dual DBs but syncs user credentials. Phase 3+ converges to Go server as sole user store, with Node.js panel delegating auth to Go server via REST API.

| Phase | Auth Model | User Store |
|-------|-----------|------------|
| Current | Dual (Node.js auth.db + Go users table) | Both, may diverge |
| Phase 1 CDAP | CDAP uses Go auth only | Go server primary for clients |
| Phase 3 | Node.js delegates to Go for user CRUD | Go server sole source |
| Long term | Single user store in Go, Node.js is pure UI | Go server |

### AD-3: CDAP Auth Message vs HTTP Login

**Decision**: CDAP clients authenticate **within the WebSocket connection** using a JSON `auth` message (not a separate HTTP `/api/login` call). The Go server validates credentials using the same `auth.VerifyPassword()` and `auth.ValidateTOTP()` functions.

**Rationale**: 
- Single connection setup (no HTTP pre-auth + WS upgrade dance)
- Works behind restrictive firewalls that only allow WS on one port
- Consistent with the "one port, one protocol" CDAP philosophy
- Server can immediately associate the WS connection with the authenticated user

```json
// Step 1: Client sends auth message
{
  "type": "auth",
  "payload": {
    "method": "user_password",
    "username": "operator1",
    "password": "...",
    "device_id": "CDAP-A7F3B210",
    "client_version": "1.0.0"
  }
}

// Step 2a: Server responds (no 2FA)
{
  "type": "auth_result",
  "payload": {
    "success": true,
    "token": "jwt_24h",
    "role": "operator",
    "device_id": "CDAP-A7F3B210"
  }
}

// Step 2b: Server responds (2FA required)
{
  "type": "auth_result",
  "payload": {
    "success": false,
    "requires_2fa": true,
    "tfa_type": "totp",
    "partial_token": "jwt_5min"
  }
}

// Step 3: Client sends 2FA code
{
  "type": "auth_2fa",
  "payload": {
    "partial_token": "jwt_5min",
    "code": "123456"
  }
}

// Step 4: Server responds with full auth
{
  "type": "auth_result",
  "payload": {
    "success": true,
    "token": "jwt_24h",
    "role": "operator",
    "device_id": "CDAP-A7F3B210"
  }
}

// Alternative: API key auth (for unattended bridges/agents)
{
  "type": "auth",
  "payload": {
    "method": "api_key",
    "key": "...",
    "device_id": "CDAP-A7F3B210"
  }
}
```

### AD-4: RustDesk Client Synchronization Strategy

**Problem**: When a Yomie native client (CDAP) runs on the same machine as a RustDesk client, or replaces it, the following must be synchronized:

| Resource | RustDesk Source | CDAP Source | Sync Strategy |
|----------|-----------------|-------------|---------------|
| Device ID | Numeric (e.g., `1340238749`) | `CDAP-` prefix | **Map**: Go server maintains `cdap_peer_id` ↔ `rustdesk_peer_id` |
| Address book | `/api/ab` (Go server) | Same `/api/ab` | **Shared**: Same user, same AB |
| Connection history | Audit log (conn events) | Audit log (conn events) | **Shared**: Same audit table |
| User account | Go `users` table | Same | **Shared**: Same credentials |
| Peers list | `peers` table | Same `peers` table | **Shared**: `device_type` column |
| Online status | Signal server in-memory map | CDAP gateway in-memory | **Merged**: Panel queries both |
| Tags/Groups | `peers.tags` | Same | **Shared**: Same field |

### AD-5: Native Client ↔ RustDesk Interoperability

**Scenario**: User A has Yomie native client. User B has RustDesk client. Can User A remote-control User B's machine?

**Decision**: **Not directly** — CDAP media channel is not compatible with RustDesk's protobuf signal/relay protocol. But the server can **bridge** the connection:

| From → To | Method | Latency | Status |
|-----------|--------|---------|--------|
| RustDesk → RustDesk | Native (signal/relay) | Low | ✅ Works now |
| CDAP → CDAP | Native (CDAP media) | Low | Phase 5 |
| CDAP → RustDesk | **Server-side protocol bridge** | Medium (+50ms) | Phase 6 |
| RustDesk → CDAP | Not needed (CDAP is superset) | — | Not planned |

The server-side protocol bridge (Phase 6) is complex and optional. The primary migration path is: install Yomie native client → device appears as both `rustdesk` AND `desktop` types → gradually deprecate RustDesk client on that machine.

### AD-6: Unified Device Identity

When a machine has both RustDesk and Yomie native clients:

```
┌─────────────────────────────────────────┐
│              Machine: PC-Design-03       │
│                                         │
│  ┌──────────────┐  ┌─────────────────┐  │
│  │ RustDesk      │  │ Yomie     │  │
│  │ Client        │  │ Native Client  │  │
│  │               │  │                │  │
│  │ ID: 892734561 │  │ ID: CDAP-D2E9F4│  │
│  │ Port: 21116   │  │ Port: 21122   │  │
│  └──────┬───────┘  └───────┬────────┘  │
│         │                   │           │
└─────────┼───────────────────┼───────────┘
          │                   │
          ▼                   ▼
┌───────────────────────────────────────┐
│           Yomie Server            │
│                                       │
│  peers table:                         │
│  ┌─────────────┬──────────┬─────────┐ │
│  │ 892734561   │ rustdesk │ linked  │ │
│  │ CDAP-D2E9F4 │ desktop  │ linked  │ │
│  └─────────────┴──────────┴─────────┘ │
│                                       │
│  peer_links table (NEW):              │
│  ┌──────────────┬───────────────────┐ │
│  │ 892734561    │ CDAP-D2E9F4      │ │
│  │ (rustdesk)   │ (desktop)        │ │
│  └──────────────┴───────────────────┘ │
│                                       │
│  Panel shows: 1 machine, 2 protocols  │
│  Admin can merge or keep separate     │
└───────────────────────────────────────┘
```

---

## Authentication & 2FA Integration

### CDAP Auth Flow (Detailed)

```
                             ┌──────────────┐
                             │  CDAP Client  │
                             │  (bridge/     │
                             │   agent)      │
                             └──────┬───────┘
                                    │
                           WS connect :21122
                                    │
                                    ▼
                            ┌───────────────┐
                            │ CDAP Gateway  │
                            │ (Go server)   │
                            └───────┬───────┘
                                    │
                    ┌───────────────┼───────────────┐
                    │               │               │
              method=api_key  method=user_password  method=device_token
                    │               │               │
                    ▼               ▼               ▼
             ┌──────────┐   ┌──────────┐   ┌──────────────┐
             │ Validate  │   │ Validate │   │ Validate     │
             │ API key   │   │ user/pass│   │ device token │
             │ (SHA256)  │   │ (PBKDF2) │   │ (enrollment) │
             └──────┬───┘   └──────┬───┘   └──────┬───────┘
                    │               │               │
                    │        ┌──────┴──────┐        │
                    │        │ TOTP check  │        │
                    │        │ if enabled  │        │
                    │        └──────┬──────┘        │
                    │               │               │
                    ▼               ▼               ▼
             ┌─────────────────────────────────────────┐
             │           Session established            │
             │  - JWT token issued (24h)               │
             │  - Role assigned (admin/operator/viewer) │
             │  - Device ID confirmed/assigned         │
             │  - WS connection upgraded to authed     │
             │  - Audit log: auth event                │
             └─────────────────────────────────────────┘
```

### Auth Methods

| Method | Use Case | 2FA | Description |
|--------|----------|-----|-------------|
| `user_password` | Interactive (operator login on desktop) | Yes (if enabled) | Same credentials as panel login |
| `api_key` | Unattended (IoT bridges, SCADA, headless) | No | API key from Go server `api_keys` table |
| `device_token` | First-time enrollment | No | One-time enrollment token (becomes API key) |

### 2FA for Native Desktop Client

The Yomie native desktop client needs a full 2FA UI:

```
┌──────────────────────────────────────┐
│       Yomie - Login             │
│                                      │
│  Server: yomie.example.com      │
│                                      │
│  Username: [operator1        ]       │
│  Password: [••••••••         ]       │
│                                      │
│  [Login]                             │
└──────────────────────────────────────┘
                │
        (server responds: requires_2fa)
                │
                ▼
┌──────────────────────────────────────┐
│       Yomie - 2FA               │
│                                      │
│  Enter authenticator code:           │
│                                      │
│  [1] [2] [3] [4] [5] [6]            │
│                                      │
│  ○ Use recovery code instead         │
│                                      │
│  [Verify]            [Cancel]        │
└──────────────────────────────────────┘
                │
        (verified → full session)
                │
                ▼
┌──────────────────────────────────────┐
│  ✓ Connected as operator1            │
│  Server: yomie.example.com      │
│  Device ID: CDAP-D2E9F4             │
│                                      │
│  This device is ready for remote     │
│  connections and management.         │
│                                      │
│  [Settings]  [Minimize to tray]      │
└──────────────────────────────────────┘
```

### 2FA Setup for CDAP Clients

CDAP clients do NOT set up 2FA themselves — 2FA is configured through the Node.js panel:

```
Panel (Node.js)                    Go Server                CDAP Client
     │                                │                         │
     │── POST /api/auth/totp/setup ──►│                         │
     │◄── {qrCode, secret} ──────────│                         │
     │                                │                         │
     │  (user scans QR in Authy)      │                         │
     │                                │                         │
     │── POST /api/auth/totp/enable ─►│                         │
     │   {code: "123456"}             │── update users table ──│
     │◄── {recoveryCodes} ───────────│                         │
     │                                │                         │
     │                                │      (next login)       │
     │                                │◄── auth {user/pass} ───│
     │                                │──► auth_result          │
     │                                │    {requires_2fa} ─────►│
     │                                │                         │
     │                                │◄── auth_2fa {code} ────│
     │                                │──► auth_result          │
     │                                │    {success, jwt} ─────►│
```

### Token Refresh for Long-Running Agents

Desktop agents and IoT bridges run 24/7. JWT tokens expire after 24h. CDAP supports token refresh:

```json
// Client sends before token expires
{
  "type": "token_refresh",
  "payload": {
    "token": "current_jwt_about_to_expire"
  }
}

// Server responds with new token (no re-auth needed)
{
  "type": "token_refreshed",
  "payload": {
    "token": "new_jwt_24h",
    "expires_at": "2026-03-21T14:30:00Z"
  }
}
```

Rules:
- Refresh only works if current token is still valid (not expired)
- Max refresh chain: 30 days (then full re-auth required)
- Admin can revoke refresh chain from panel (force re-login)
- API key auth does not need refresh (keys don't expire by default)

---

## RustDesk Client Synchronization

### Problem Statement

A Yomie deployment typically has a mix of:
- **Existing RustDesk clients** — already deployed on workstations, using RustDesk protocol on port 21116
- **New Yomie native clients** (CDAP) — being rolled out, using CDAP on port 21122
- **IoT/SCADA bridges** (CDAP) — new devices via CDAP

These must coexist in the same panel, same device list, same address books, same permissions.

### Sync Points

#### 1. Unified Peer Table

Both RustDesk and CDAP devices live in the same `peers` table:

```sql
-- Existing columns (unchanged)
id TEXT PRIMARY KEY,           -- "1340238749" (RustDesk) or "CDAP-D2E9F4" (CDAP)
uuid TEXT,
pk BLOB,
hostname TEXT,
os TEXT,
...

-- Existing CDAP columns (from Phase 1)
device_type TEXT DEFAULT 'rustdesk',  -- rustdesk, scada, iot, os_agent, desktop, ...
manifest_json TEXT,
bridge_id TEXT,
cdap_session_id TEXT,

-- NEW: Linking columns
linked_peer_id TEXT,           -- Cross-reference for dual-client machines
link_type TEXT,                -- "auto" (hostname match) or "manual" (admin linked)
auth_user TEXT,                -- Username who logged in on this device
last_auth_at TIMESTAMPTZ       -- Last CDAP/RustDesk login time
```

#### 2. Address Book Convergence

RustDesk clients and CDAP clients share the same address book per user:

```
User "operator1" logs in from:
  ├── Node.js panel → sees all devices in panel
  ├── RustDesk client → GET /api/ab → sees address book
  └── CDAP native client → GET /api/ab (same endpoint) → sees address book

Address book contains:
  ├── RustDesk device IDs (numeric)
  └── CDAP device IDs (CDAP- prefix)

Both client types can:
  ├── Add devices to address book
  ├── Create tags/groups
  └── Sync across devices
```

The Go server's `/api/ab` endpoint already works for RustDesk clients. CDAP clients use the **same endpoint** with the same JWT token, so address books are automatically synchronized.

#### 3. Connection History

When a CDAP client connects to another device (remote desktop):

```json
// Audit log entry (same format as RustDesk connections)
{
  "action": "connection",
  "details": {
    "from_id": "CDAP-D2E9F4",
    "from_type": "desktop",
    "to_id": "892734561",
    "to_type": "rustdesk",
    "protocol": "cdap_media",
    "initiated_by": "operator1",
    "duration_sec": 1847,
    "timestamp": "2026-03-19T14:30:00Z"
  }
}
```

The Node.js panel's connection history view shows both RustDesk and CDAP connections in the same timeline.

#### 4. Online Status Merging

The panel needs to query both connection pools:

```
Node.js Panel
     │
     ├── GET /api/peers (Go server)
     │    └── Go enriches with:
     │         ├── RustDesk live status (signal server in-memory map)
     │         └── CDAP live status (CDAP gateway in-memory map)
     │
     └── Shows unified device list with live status from both protocols
```

Implementation: `handleListPeers` in Go server already overlays live status from signal peer map. Extended to also check CDAP gateway's active connections map.

#### 5. Linked Device View

When a machine runs both RustDesk and Yomie native clients:

```
Panel Device List:
┌──────────────────────────────────────────────────────────────┐
│  PC-Design-03                                    [Linked]    │
│  ├── 🖥️ 892734561   | RustDesk  | Online | Remote Desktop   │
│  └── 📱 CDAP-D2E9F4 | Desktop   | Online | Desktop + Mgmt   │
│                                                              │
│  srv-prod-01                                                 │
│  └── 💻 CDAP-F9A2B1 | OS Agent  | Online | System Management│
│                                                              │
│  Boiler Room PLC                                             │
│  └── 🏭 CDAP-A7F3B2 | SCADA     | Online | Widgets          │
└──────────────────────────────────────────────────────────────┘
```

Auto-linking algorithm:
1. Same hostname + same user → auto-link
2. Same IP (within 5 min window) + same hostname → suggest link
3. Admin can manually link/unlink from panel

---

## Implementation Phases

### Phase 0: Preparation (1-2 days)

| # | Task | Effort | Files |
|---|------|--------|-------|
| 0.1 | Add `device_type`, `manifest_json`, `linked_peer_id`, `auth_user` columns to peers | 0.5d | `db/sqlite.go`, `db/postgres.go` |
| 0.2 | Add `peer_links` table for device linking | 0.5d | `db/sqlite.go`, `db/postgres.go` |
| 0.3 | Update `handleListPeers` to return `device_type` field | 0.5d | `api/server.go` |
| 0.4 | Add device type filter to panel device list | 0.5d | `web-nodejs/views/`, `web-nodejs/routes/` |

**Gate**: Device list shows `device_type` column. All existing RustDesk devices show as `rustdesk`.

---

### Phase 1: CDAP Gateway Core (5-7 days)

| # | Task | Effort | Files |
|---|------|--------|-------|
| 1.1 | Create `cdap/` package with Gateway struct | 1d | `cdap/gateway.go` (NEW) |
| 1.2 | WebSocket server on port 21122 (`gorilla/websocket`) | 1d | `cdap/gateway.go` |
| 1.3 | Auth handler — `user_password` method (reuse `auth.VerifyPassword`) | 0.5d | `cdap/auth.go` (NEW) |
| 1.4 | Auth handler — `api_key` method (reuse `auth.ValidateAPIKey`) | 0.5d | `cdap/auth.go` |
| 1.5 | 2FA handler — TOTP flow within WebSocket (reuse `auth.ValidateTOTP`) | 0.5d | `cdap/auth.go` |
| 1.6 | Rate limiting on auth messages (reuse `ratelimit.IPLimiter`) | 0.5d | `cdap/auth.go` |
| 1.7 | Manifest parser + validation (JSON Schema) | 1d | `cdap/manifest.go` (NEW) |
| 1.8 | Device registration — write to `peers` table with `device_type` | 0.5d | `cdap/handler.go` (NEW) |
| 1.9 | Heartbeat handler — widget value storage + online status | 0.5d | `cdap/handler.go` |
| 1.10 | Connection lifecycle — reconnect ID persistence by serial | 0.5d | `cdap/handler.go` |
| 1.11 | Wire into `main.go` — start CDAP gateway alongside signal/relay | 0.5d | `main.go` |

**Gate**: Python bridge connects, authenticates (including 2FA), registers device, sends heartbeat. Device appears in `peers` table as `device_type=scada`.

---

### Phase 2: Panel Widget Rendering (5-7 days)

| # | Task | Effort | Files |
|---|------|--------|-------|
| 2.1 | REST endpoint: `GET /api/cdap/devices/{id}/manifest` | 0.5d | `api/server.go` or `cdap/api.go` |
| 2.2 | REST endpoint: `GET /api/cdap/devices/{id}/state` (current widget values) | 0.5d | `cdap/api.go` |
| 2.3 | REST endpoint: `POST /api/cdap/devices/{id}/command` (send command) | 0.5d | `cdap/api.go` |
| 2.4 | WebSocket push: widget state changes → panel (via existing event bus) | 1d | `cdap/handler.go`, `events/` |
| 2.5 | Panel: CDAP device detail page (EJS template) | 1d | `web-nodejs/views/cdap-device.ejs` (NEW) |
| 2.6 | Panel: Widget renderer — `toggle`, `gauge`, `button`, `led` | 2d | `web-nodejs/public/js/cdap-widgets.js` (NEW) |
| 2.7 | Panel: Command sending UI (confirm dialogs, cooldowns) | 1d | `web-nodejs/public/js/cdap-commands.js` (NEW) |
| 2.8 | Panel: Device list shows device type icons + filter | 0.5d | `web-nodejs/views/devices.ejs`, `devices.js` |
| 2.9 | i18n keys for CDAP widgets and device types (EN + PL) | 0.5d | `web-nodejs/lang/en.json`, `pl.json` |

**Gate**: Panel shows CDAP device detail with live-updating gauges, working toggle switches, functioning buttons with confirmations.

---

### Phase 3: Security Hardening + Auth Convergence + Device Revocation (6-8 days)

| # | Task | Effort | Files |
|---|------|--------|-------|
| 3.1 | TLS auto-detection on CDAP port (reuse `config.DualModeListener`) | 0.5d | `cdap/gateway.go` |
| 3.2 | RBAC per-widget permissions enforcement | 1d | `cdap/auth.go`, `cdap/handler.go` |
| 3.3 | Command audit logging (every command → audit ring buffer) | 0.5d | `cdap/handler.go`, `audit/` |
| 3.4 | Device token enrollment (one-time tokens for new devices) | 1d | `cdap/auth.go`, `db/` |
| 3.5 | Token refresh for long-running agents (30-day chain) | 0.5d | `cdap/auth.go` |
| 3.6 | Auth delegation: Node.js panel uses Go `/api/auth/` for user CRUD | 1d | `web-nodejs/services/authService.js` (NEW) |
| 3.7 | Panel: user management through Go API (password change syncs to Go) | 0.5d | `web-nodejs/routes/auth.routes.js` |
| 3.8 | **Device Revocation**: CDAP `revoke` + `suspend` message handlers in gateway | 1d | `cdap/revocation.go` (NEW), `cdap/gateway.go` |
| 3.9 | **Connection close on delete**: Close `TCPConn`/`WSConn` when peer removed from map | 0.5d | `peer/map.go`, `api/server.go` |
| 3.10 | **Blocklist on revoke**: Auto-add ID to `security.Blocklist` on `?revoke=true` | 0.5d | `api/server.go`, `security/blocklist.go` |
| 3.11 | **Cascade delete**: Revoke `linked_peer_id` when cascade option selected | 0.5d | `cdap/revocation.go`, `api/server.go` |
| 3.12 | **Panel revocation UI**: Delete dialog with wipe/blocklist/cascade checkboxes | 1d | `web-nodejs/views/`, `public/js/devices.js` |
| 3.13 | **EventPeerRevoked**: New event type on event bus for revocation audit trail | 0.25d | `events/bus.go` |

**Gate**: Widget RBAC enforced (operator can read gauge but not trigger emergency stop without permission). 2FA works end-to-end for CDAP clients. Password changed in panel reflects in CDAP login. Deleting a CDAP device sends `revoke` message → client wipes config and disconnects. Cascade delete revokes linked devices.

---

### Phase 4: Advanced Widgets + RustDesk Sync (5-7 days)

| # | Task | Effort | Files |
|---|------|--------|-------|
| 4.1 | Widget types: `chart`, `select`, `slider`, `text` | 2d | `web-nodejs/public/js/cdap-widgets.js` |
| 4.2 | Widget types: `table` (dynamic rows, sortable) | 1d | `cdap-widgets.js` |
| 4.3 | Widget type: `terminal` (WebSocket shell relay) | 2d | `cdap/terminal.go` (NEW), `cdap-widgets.js` |
| 4.4 | Device linking — auto-detect + manual link in panel | 1d | `cdap/linking.go` (NEW), panel views |
| 4.5 | Unified status overlay — CDAP status merged into `handleListPeers` | 0.5d | `api/server.go` |
| 4.6 | Address book sync — CDAP clients use same `/api/ab` endpoint | 0.5d | `cdap/api.go` (proxy or direct) |
| 4.7 | Connection history — CDAP events in same audit format | 0.5d | `cdap/handler.go`, `web-nodejs/routes/` |

**Gate**: Full 10+ widget types working. Linked devices show as one machine in panel. Address book shared between RustDesk and CDAP clients.

---

### Phase 5: Media Channel — Remote Desktop (10-15 days)

| # | Task | Effort | Files |
|---|------|--------|-------|
| 5.1 | Binary frame mux/demux in CDAP gateway | 2d | `cdap/media.go` (NEW) |
| 5.2 | Media session establishment (REST `/api/cdap/devices/{id}/connect`) | 1d | `cdap/api.go` |
| 5.3 | Binary frame relay (viewer ↔ device, E2E opaque) | 1d | `cdap/media.go` |
| 5.4 | E2E key exchange (X25519 → XSalsa20-Poly1305 via control messages) | 1d | `cdap/crypto.go` (NEW) |
| 5.5 | Video channel: codec negotiation + keyframe request handling | 1d | `cdap/media.go` |
| 5.6 | Panel: WebCodecs/Canvas desktop viewer widget | 3d | `web-nodejs/public/js/cdap-desktop.js` (NEW) |
| 5.7 | Panel: input forwarding (keyboard/mouse → binary frames) | 1d | `cdap-desktop.js` |
| 5.8 | Panel: clipboard sync widget | 1d | `cdap-desktop.js` |
| 5.9 | Panel: file transfer two-pane browser | 2d | `web-nodejs/public/js/cdap-files.js` (NEW) |
| 5.10 | Audio channel: Opus decode/play in browser | 1d | `cdap-desktop.js` |
| 5.11 | Cursor channel: custom cursor rendering | 0.5d | `cdap-desktop.js` |
| 5.12 | Adaptive quality (bitrate/fps adjustments) | 1d | `cdap/media.go` |
| 5.13 | Multi-monitor support (display index routing) | 0.5d | `cdap/media.go`, `cdap-desktop.js` |

**Gate**: Panel can open remote desktop session to a CDAP desktop device. Video, audio, input, clipboard, and file transfer all work through the binary media channel with E2E encryption.

---

### Phase 6: Native Yomie Desktop Agent (10-15 days)

| # | Task | Effort | Files |
|---|------|--------|-------|
| 6.1 | Agent binary scaffold (Rust or Go) — systemd/service installer | 2d | `yomie-agent/` (NEW repo or dir) |
| 6.2 | CDAP client library — auth + heartbeat + manifest | 2d | `yomie-agent/cdap/` |
| 6.3 | Screen capture — DXGI (Windows), X11/PipeWire (Linux) | 2d | `yomie-agent/capture/` |
| 6.4 | Video encoder — H.264/VP9 hardware-accelerated | 2d | `yomie-agent/encoder/` |
| 6.5 | Audio capture — WASAPI (Windows), PulseAudio (Linux) | 1d | `yomie-agent/audio/` |
| 6.6 | Input injection — keyboard/mouse (platform-specific) | 1d | `yomie-agent/input/` |
| 6.7 | Clipboard monitor — text/image sync | 1d | `yomie-agent/clipboard/` |
| 6.8 | File transfer — chunk read/write with resume | 1d | `yomie-agent/files/` |
| 6.9 | System widgets — CPU/RAM/disk/services/processes | 1d | `yomie-agent/sysinfo/` |
| 6.10 | Login UI — username/password + 2FA dialog | 1d | `yomie-agent/ui/` |
| 6.11 | Tray icon + auto-start | 0.5d | `yomie-agent/ui/` |
| 6.12 | Update from ALL-IN-ONE scripts (install/update support) | 1d | `yomie.sh`, `yomie.ps1` |

**Gate**: Yomie agent installs on Windows/Linux, logs in with 2FA, appears as `desktop` type in panel, supports remote desktop + system management widgets.

---

### Phase 7: Bridge Ecosystem + Polish (5-7 days)

| # | Task | Effort | Files |
|---|------|--------|-------|
| 7.1 | Python bridge SDK (pip-installable) | 1d | `sdks/python/` (NEW) |
| 7.2 | Reference bridge: Modbus TCP | 1d | `bridges/modbus/` (NEW) |
| 7.3 | Reference bridge: SNMP | 1d | `bridges/snmp/` (NEW) |
| 7.4 | Reference bridge: REST/HTTP (generic webhook → CDAP) | 1d | `bridges/rest/` (NEW) |
| 7.5 | Dashboard: CDAP device type counters + overview cards | 1d | Panel views |
| 7.6 | Alert system: threshold-based from manifest definitions | 1d | `cdap/alerts.go` (NEW) |
| 7.7 | Documentation site / updated README | 1d | `docs/` |

**Gate**: Three working reference bridges. Alert system notifies on threshold breach. Dashboard shows CDAP devices alongside RustDesk.

---

## File-Level Change Map

### Go Server (`yomie-server/`)

| File | Action | Phase | Description |
|------|--------|-------|-------------|
| `main.go` | Modify | 1 | Start CDAP gateway, pass config |
| `config/config.go` | Modify | 1 | Add `CDAPPort`, `CDAPTLSEnabled()` |
| `cdap/gateway.go` | **NEW** | 1 | WebSocket server, connection lifecycle |
| `cdap/auth.go` | **NEW** | 1 | Auth handler (user/pass, API key, 2FA, device token) |
| `cdap/handler.go` | **NEW** | 1 | Register, heartbeat, state_update, command routing |
| `cdap/manifest.go` | **NEW** | 1 | Manifest parsing + JSON validation |
| `cdap/api.go` | **NEW** | 2 | REST endpoints for panel (manifest, state, command) |
| `cdap/terminal.go` | **NEW** | 4 | Terminal widget WebSocket relay |
| `cdap/linking.go` | **NEW** | 4 | Device auto-linking logic |
| `cdap/media.go` | **NEW** | 5 | Binary frame mux/demux, media session, relay |
| `cdap/crypto.go` | **NEW** | 5 | X25519 key exchange for E2E media |
| `cdap/revocation.go` | **NEW** | 3 | Revoke/suspend message sending, cascade logic, conn close |
| `cdap/alerts.go` | **NEW** | 7 | Threshold-based alert evaluation |
| `db/database.go` | Modify | 0-1 | Add CDAP-related methods to interface |
| `db/sqlite.go` | Modify | 0-1 | Implement CDAP methods, add columns/tables |
| `db/postgres.go` | Modify | 0-1 | Same for PostgreSQL |
| `peer/map.go` | Modify | 3 | Close TCP/WS connections on `Remove()` |
| `api/server.go` | Modify | 2-4 | CDAP REST routes, status overlay, `?revoke=true` in delete |
| `events/bus.go` | Modify | 3 | Add `EventPeerRevoked` event type |
| `security/blocklist.go` | Modify | 3 | Auto-add ID on revocation |
| `auth/` (existing) | No change | — | Reused by CDAP auth (no modification needed) |

### Node.js Console (`web-nodejs/`)

| File | Action | Phase | Description |
|------|--------|-------|-------------|
| `views/cdap-device.ejs` | **NEW** | 2 | CDAP device detail page with widget panel |
| `views/devices.ejs` | Modify | 0 | Add device type column, filter, icons |
| `public/js/cdap-widgets.js` | **NEW** | 2 | Widget renderer (all 13 types) |
| `public/js/cdap-commands.js` | **NEW** | 2 | Command sending, confirmation, cooldown |
| `public/js/cdap-desktop.js` | **NEW** | 5 | Remote desktop viewer (WebCodecs + Canvas) |
| `public/js/cdap-files.js` | **NEW** | 5 | Two-pane file browser |
| `public/css/cdap.css` | **NEW** | 2 | Widget styles, device type icons |
| `routes/cdap.routes.js` | **NEW** | 2 | CDAP panel routes (proxy to Go server) |
| `services/authService.js` | **NEW** | 3 | Auth delegation to Go server |
| `lang/en.json` | Modify | 2+ | CDAP i18n keys |
| `lang/pl.json` | Modify | 2+ | CDAP i18n keys |
| `services/betterdeskApi.js` | Modify | 2 | Add CDAP API methods |

### ALL-IN-ONE Scripts

| File | Action | Phase | Description |
|------|--------|-------|-------------|
| `yomie.sh` | Modify | 6 | CDAP port in firewall, agent install option |
| `yomie.ps1` | Modify | 6 | Same for Windows |
| `yomie-docker.sh` | Modify | 1 | Expose port 21122 in docker-compose |
| `docker-compose.yml` | Modify | 1 | Add port 21122 mapping |
| `Dockerfile` | Modify | 1 | Expose 21122 |

---

## Testing Strategy

### Unit Tests (Go)

| Test | Phase | Validates |
|------|-------|-----------|
| `cdap/auth_test.go` | 1 | Auth message parsing, password verify, API key, 2FA flow |
| `cdap/manifest_test.go` | 1 | Manifest validation (valid, invalid, edge cases) |
| `cdap/handler_test.go` | 1 | Register, heartbeat, state_update, command routing |
| `cdap/media_test.go` | 5 | Binary frame mux/demux, session pairing |
| `cdap/linking_test.go` | 4 | Auto-link algorithm, unlink |
| `cdap/revocation_test.go` | 3 | Revoke message sent, conn closed, config wipe, cascade |

### Integration Tests

| Test | Phase | Setup | Validates |
|------|-------|-------|-----------|
| Bridge auth flow | 1 | Go server + Python bridge | WS connect → auth → register → heartbeat |
| 2FA end-to-end | 1 | Go server + test TOTP | Auth → 2FA required → code verify → session |
| Panel widget rendering | 2 | Full stack | Bridge connects → panel shows widgets → values update |
| Command round-trip | 2 | Full stack | Panel sends command → bridge receives → executes → response |
| Linked devices | 4 | Go server + RustDesk + CDAP | Both clients online → panel shows linked view |
| Device revocation | 3 | Go server + CDAP bridge | Delete → revoke msg → bridge disconnects + wipes config |
| Cascade revocation | 3 | Go server + RustDesk + CDAP | Delete CDAP → linked RustDesk also revoked + blocked |
| Media relay | 5 | Go server + 2 CDAP clients | Video frames relayed with E2E encryption |

### Load Tests

| Test | Phase | Target |
|------|-------|--------|
| 100 concurrent CDAP devices | 2 | Gateway handles 100 WS connections, 1000 msg/s |
| 10 concurrent media sessions | 5 | Binary relay at 10 Mbps aggregate without frame loss |
| 1000 widget updates/sec | 2 | Panel receives updates via WS push without lag |

---

## Migration & Backward Compatibility

### Zero Breaking Changes

- All existing RustDesk clients continue working unchanged
- All existing Node.js panel features unchanged
- All existing API endpoints unchanged
- CDAP is strictly additive — new port, new protocol, new device types

### Migration Path for Organizations

```
Step 1: Update Yomie server (Phase 1-4)
  └── CDAP gateway starts on port 21122
  └── All existing devices still work

Step 2: Deploy IoT/SCADA bridges (Phase 2+)
  └── New device types appear in panel
  └── RustDesk devices unaffected

Step 3: Deploy Yomie native agent on select machines (Phase 6)
  └── Machine shows two entries (RustDesk + CDAP)
  └── Admin can link them in panel

Step 4: Gradually replace RustDesk client with native agent
  └── Uninstall RustDesk client
  └── CDAP agent provides remote desktop + management
  └── One device entry per machine

Step 5 (optional): Disable RustDesk protocol
  └── Only for environments fully migrated to CDAP
  └── Signal/relay servers still available for backward compat
```

### Rollback

Every phase is independent. If Phase 5 (media) has issues, Phases 1-4 (widgets, auth, sync) continue working. CDAP gateway can be disabled entirely by removing the `--cdap-port` flag — zero impact on RustDesk protocol.

---

## Risk Assessment

| Risk | Impact | Likelihood | Mitigation |
|------|--------|------------|------------|
| WebSocket scalability at 1000+ devices | High | Medium | Connection pooling, per-device message throttle |
| Auth DB divergence (Node.js vs Go) | High | High | Phase 3 convergence, sync checks |
| Binary frame relay performance for desktop | High | Low | E2E opaque (zero decode), tested architecture |
| Native agent cross-platform complexity | High | High | Start with one platform (Linux or Windows), add second |
| Bridge SDK maintenance burden | Medium | Medium | Minimal SDKs (~200 LOC), auto-generate from spec |
| RustDesk protocol changes break existing | Medium | Low | Yomie server frozen at RustDesk 1.3.x compat |
| 2FA lockout (lost phone, no recovery codes) | Medium | Medium | Admin can disable 2FA from panel, recovery codes backup reminder |
| CDAP spec changes during development | Medium | Medium | Versioned manifests, backward compat |
| Revocation message not delivered (device offline) | Medium | High | Blocklist + soft-delete ensure re-registration fails regardless; `revoke` is best-effort optimization |
| Cascade delete accidental scope | High | Low | Cascade is opt-in checkbox, admin must explicitly confirm; audited |
| RustDesk client cannot be config-wiped | Medium | N/A | Protocol limitation; blocklist prevents re-registration; Yomie native client supports full wipe |

---

## Timeline Summary

| Phase | Duration | Cumulative | Key Deliverable |
|-------|----------|------------|-----------------|
| 0: Preparation | 1-2 days | 1-2 days | DB schema, device type column |
| 1: Gateway Core | 5-7 days | 6-9 days | CDAP auth + registration working |
| 2: Panel Widgets | 5-7 days | 11-16 days | Widgets visible and interactive in panel |
| 3: Security + Auth + Revocation | 6-8 days | 17-25 days | Full RBAC + 2FA + auth convergence + device revocation |
| 4: Advanced + Sync | 5-7 days | 22-32 days | Full widget set + RustDesk sync |
| 5: Media Channel | 10-15 days | 32-47 days | Remote desktop via CDAP |
| 6: Native Agent | 10-15 days | 42-62 days | Yomie desktop agent binary |
| 7: Ecosystem | 5-7 days | 47-69 days | Bridge SDKs + reference bridges |

**MVP (Phases 0-2)**: ~16 days → CDAP devices with widgets in panel  
**Production (Phases 0-4)**: ~32 days → Secure, synced, full widget set + revocation  
**Full Stack (Phases 0-7)**: ~69 days → Native client + bridge ecosystem

# 🚀 Yomie v3 — Ultimate Remote Desktop & CDAP Solution

<div align="center">

<img src="yomie.png" alt="Yomie Logo" width="320">

<br><br>

![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)
![Go](https://img.shields.io/badge/Go-1.21+-00ADD8.svg)
![Node.js](https://img.shields.io/badge/Node.js-18+-339933.svg)
![Version](https://img.shields.io/badge/version-3.0.0-brightgreen.svg)
![Security](https://img.shields.io/badge/Security-TLS%20%2B%20NaCl%20%2B%20TOTP%20%2B%20E2EE-green.svg)
![Database](https://img.shields.io/badge/DB-SQLite%20%2B%20PostgreSQL-blue.svg)
![CDAP](https://img.shields.io/badge/CDAP-v1.0-orange.svg)
![i18n](https://img.shields.io/badge/i18n-25%2B%20languages-purple.svg)

**A clean-room RustDesk-compatible server written in Go — single binary replacing hbbs + hbbr — with full protocol support, TLS everywhere, PostgreSQL backend, CDAP (Custom Device API Protocol) for IoT/SCADA/network devices, and a modern Node.js web management console.**

[Architecture](#-architecture) • [Installation](#-installation) • [Configuration](#-configuration) • [Security](#-security-architecture) • [API](#-api-reference) • [Troubleshooting](#-troubleshooting)

</div>

---

## ✅ End-to-End Encryption — Fully Working

> **E2E encryption between RustDesk clients is fully functional.** Both P2P (punch-hole) and relay sessions establish NaCl-encrypted channels with proper `SignedId` + `PublicKey` handshake. The green lock indicator appears in the RustDesk client for all connection modes.
>
> Additionally, you can enable **TLS on relay ports** (`--tls-relay` / `TLS_RELAY=Y`) for an extra transport-level encryption layer on top of the E2E channel.

---

## 📋 Table of Contents

- [Overview](#-overview)
- [Architecture](#-architecture)
- [Yomie Go Server](#-yomie-go-server)
  - [Protocol Implementation](#protocol-implementation)
  - [Cryptography](#cryptography)
  - [Database Backends](#database-backends)
  - [TLS Support](#tls-support)
  - [Rate Limiting & DDoS Protection](#rate-limiting--ddos-protection)
  - [Enrollment & Device Tokens](#enrollment--device-tokens)
- [Web Console (Node.js)](#-web-console-nodejs)
- [Security Architecture](#-security-architecture)
- [Installation](#-installation)
  - [Linux](#linux)
  - [Windows](#windows)
  - [Docker](#docker)
- [RustDesk Client Configuration](#-rustdesk-client-configuration)
  - [Desktop Client Login](#desktop-client-login)
  - [Enabling Pro Features](#enabling-pro-features)
- [TLS / SSL Certificates](#-tls--ssl-certificates)
- [Configuration Reference](#-configuration-reference)
- [API Reference](#-api-reference)
- [Migration Guide](#-migration-guide)
- [Monitoring & Metrics](#-monitoring--metrics)
- [Troubleshooting](#-troubleshooting)
- [E2E Encryption](#-e2e-encryption)
- [Chat E2E Encryption](#-chat-e2e-encryption)
- [Unattended Access & Wake-on-LAN](#-unattended-access--wake-on-lan)
- [Technology Stack](#-technology-stack)
- [Contributing](#-contributing)
- [License](#-license)

---

## 🌟 Overview

**Yomie** is a complete RustDesk infrastructure solution consisting of two main components:

1. **Yomie Server** — A clean-room Go implementation that replaces both `hbbs` (signal) and `hbbr` (relay) with a **single binary**. It implements the full RustDesk wire protocol, including UDP/TCP/WebSocket signal, TCP/WebSocket relay, NaCl secure handshake, and a comprehensive HTTP REST API.

2. **Yomie Console** — A Node.js (Express.js) web management panel with device monitoring, TOTP 2FA, RBAC, address book sync, and RustDesk Client API.

### Why Yomie?

| Feature | Original RustDesk Server | Yomie Server |
|---------|------------------------|-------------------|
| **Binaries** | 2 (hbbs + hbbr) | **1 single binary** |
| **Language** | Rust | Go (pure Go, no CGO) |
| **Database** | SQLite only | **SQLite + PostgreSQL** |
| **TLS** | Not built-in | **TLS everywhere** (auto-detect plain/TLS on same port) |
| **API** | Minimal | **Full REST API** with JWT + API keys |
| **Status tracking** | Binary (online/offline) | **4-tier** (Online/Degraded/Critical/Offline) |
| **Rate limiting** | None | **IP, bandwidth, connection** limits |
| **Metrics** | None | **Prometheus** exposition format |
| **Admin console** | None | **TCP admin** (telnet/netcat) |
| **Device enrollment** | Open only | **Open/Managed/Locked** modes |
| **Audit trail** | None | **Ring-buffer + JSON file** audit logging |
| **Hot reload** | Restart required | **SIGHUP** config reload |
| **Multi-instance** | File-based only | **PostgreSQL LISTEN/NOTIFY** for real-time sync |
| **Web panel** | None | **Full Node.js console** with TOTP 2FA |
| **User roles** | Single admin | **4-tier RBAC** (Admin, Operator, Viewer, Pro) |
| **Chat** | None | **E2E encrypted** operator↔device chat |
| **Wake-on-LAN** | None | **Built-in WOL** via magic packet |
| **Access policies** | None | **Unattended access** schedules + operator restrictions |

> **🤖 AI-Assisted Development**: This project was developed with significant assistance from AI coding tools (Claude/GitHub Copilot). All code has been reviewed, tested, and validated for production use.

---

## 🏗️ Architecture

```
RustDesk Desktop/Mobile Clients
  │
  ├── UDP (:21116) ──────► Signal Server ──► RegisterPeer, PunchHole, RequestRelay
  ├── TCP (:21116) ──────► Signal Server ──► NaCl Secure Handshake → encrypted channel
  ├── TCP (:21115) ──────► NAT Test      ──► TestNatRequest, OnlineRequest, ConfigUpdate
  ├── WS  (:21118) ──────► WS Signal     ──► WebSocket-based signal (web clients)
  │
  ├── TCP (:21117) ──────► Relay Server  ──► UUID pairing → bidirectional io.Copy pipe
  ├── WS  (:21119) ──────► WS Relay      ──► WebSocket relay (web clients)
  │
  └── HTTP (:21121) ─────► Client API    ──► Login, address book sync, heartbeat, sysinfo
                                              (WAN-facing, 7-layer security stack)

Admin / Web Console
  │
  ├── HTTP  (:21114) ────► REST API      ──► JWT/API-key auth → CRUD endpoints
  ├── WS    (:21114) ────► Event Stream  ──► Real-time peer status push
  ├── TCP   (admin)  ────► Admin Console ──► telnet/netcat management (127.0.0.1 only)
  └── HTTP  (:5000)  ────► Web Console   ──► Node.js + Express + EJS (LAN)
                                              TOTP 2FA, RBAC, device management

Database Layer
  │
  ├── SQLite  ──► Pure Go (modernc.org/sqlite), WAL mode, single binary
  └── PostgreSQL ──► pgx/v5 + pgxpool, LISTEN/NOTIFY, row-level locking
```

### Port Map

| Port | Protocol | Service | Direction |
|------|----------|---------|-----------|
| **21114** | HTTP(S) | REST API (Go server) | LAN |
| **21115** | TCP | NAT type test + OnlineRequest | WAN |
| **21116** | TCP + UDP | Signal Server (registration, punch hole) | WAN |
| **21117** | TCP | Relay Server (bidirectional stream) | WAN |
| **21118** | WS(S) | WebSocket Signal | WAN |
| **21119** | WS(S) | WebSocket Relay | WAN |
| **21121** | HTTP | RustDesk Client API — Node.js (login, AB sync, heartbeat) | WAN |
| **5000** | HTTP | Web Console (admin panel) | LAN |

> All TCP/WS ports support **dual-mode TLS** — plain and TLS on the same port with automatic detection.

---

## 🔧 Yomie Go Server

The Go server (`yomie-server/`) is a ~20,000 LOC clean-room implementation of the RustDesk signal and relay protocol. It compiles to a **single static binary** with no external dependencies (pure Go, no CGO required).

### Server Modes

```bash
# Default: run everything (signal + relay + API + admin)
./yomie-server -mode all

# Signal only (no relay)
./yomie-server -mode signal

# Relay only
./yomie-server -mode relay
```

### Protocol Implementation

The server implements the complete RustDesk rendezvous/relay protocol:

#### Signal Protocol (UDP/TCP/WS)

| Message | Direction | Description |
|---------|-----------|-------------|
| `RegisterPeer` | Client → Server | Heartbeat registration (~12s interval), updates in-memory peer map |
| `RegisterPeerResponse` | Server → Client | Suggested heartbeat interval, request online status |
| `RegisterPk` | Client → Server | Public key registration with UUID consistency check |
| `RegisterPkResponse` | Server → Client | Registration result |
| `PunchHoleRequest` | Client A → Server | Request hole-punch to Client B |
| `PunchHole` | Server → Client B | Forward punch request with Client A's address |
| `PunchHoleSent` | Client B → Server | Confirm punch sent |
| `PunchHoleResponse` | Server → Client A | Relay response with Client B's NAT info |
| `RequestRelay` | Client → Server | Request relay session |
| `RelayResponse` | Server → Clients | Relay server address + UUID for pairing |
| `FetchLocalAddr` / `LocalAddr` | Bidirectional | LAN address exchange for direct connections |
| `TestNatRequest` | Client → Server (:21115) | NAT type detection |
| `TestNatResponse` | Server → Client | NAT type + `ConfigUpdate` (relay/rendezvous servers) |
| `OnlineRequest` | Client → Server (:21115) | Bulk online status query (bitmask response) |

#### Relay Protocol (TCP/WS)

The relay is a **pure opaque byte pipe**:

1. Client A connects, sends `RequestRelay{uuid: "..."}` → registered as pending
2. Client B connects with the same UUID → pair found
3. Bidirectional `io.Copy` begins immediately (relay is a pure opaque byte pipe)
4. Timeouts: 30s pairing, 30s idle (extended on activity via `idleTimeoutConn`)

The relay does **not** parse, inspect, or modify traffic between paired clients. E2E encryption is entirely between the two RustDesk clients.

#### Wire Protocol (codec/)

Matches `hbb_common::bytes_codec::BytesCodec`:

- **TCP**: Variable-length framing — bottom 2 bits of header = header length - 1, remaining bits = payload length (little-endian). Supports 1-4 byte headers (max frame: 64 KB)
- **UDP**: Raw protobuf (no framing)
- **WebSocket**: Raw protobuf per binary WS frame

### Cryptography

#### Server-Client Secure Handshake (NaCl)

```
Server                                          Client
  │                                                │
  │  KeyExchange{keys: [Ed25519_sign(Cv_pub)]}     │
  │ ──────────────────────────────────────────────► │
  │                                                │
  │  KeyExchange{keys: [client_cv_pub,             │
  │               nacl_box(symmetric_key)]}         │
  │ ◄────────────────────────────────────────────── │
  │                                                │
  │  ═══ All traffic encrypted with NaCl secretbox ═══
```

1. Server generates Ed25519 keypair → derives Curve25519 public key → signs with Ed25519
2. Client verifies signature → generates its own Curve25519 keypair
3. Client encrypts a random symmetric key using NaCl box (Curve25519 DH)
4. All subsequent messages use NaCl secretbox with sequential nonces
5. **Backward compatible**: Auto-detects old (plain) vs new (encrypted) clients

#### Key Storage

- **Private key**: `id_ed25519` file (auto-generated if missing)
- **Public key**: `id_ed25519.pub` (base64-encoded, same format as original RustDesk)
- **Key format**: Standard Ed25519 (32-byte seed), compatible with existing RustDesk clients

#### Password Hashing

- **Algorithm**: PBKDF2-HMAC-SHA256
- **Iterations**: 100,000
- **Salt**: 16-byte cryptographically random
- **Comparison**: Constant-time (`subtle.ConstantTimeCompare`)

### Database Backends

#### SQLite (Default)

```bash
./yomie-server -db ./db_v2.sqlite3
```

- **Driver**: `modernc.org/sqlite` — pure Go, no CGO required
- **Journal mode**: WAL (Write-Ahead Logging) for concurrent reads
- **Foreign keys**: Enabled by default
- **Connection limit**: 1 (SQLite single-writer constraint)
- **Write serialization**: `sync.RWMutex` around all write operations
- **Tables**: `peers`, `server_config`, `id_change_history`, `users`, `api_keys`, `device_tokens`

#### PostgreSQL

```bash
./yomie-server -db "postgres://user:password@localhost:5432/yomie?sslmode=disable"
```

- **Driver**: `pgx/v5` with `pgxpool` connection pooling
- **Pool size**: Configurable via `pool_max_conns` DSN parameter (default: 10)
- **Native types**: `BOOLEAN`, `BYTEA`, `TIMESTAMPTZ`, `BIGSERIAL`
- **Row-level locking**: `SELECT ... FOR UPDATE` in `ChangePeerID` (replaces SQLite global mutex)
- **LISTEN/NOTIFY**: Real-time cross-instance event push for multi-server deployments
- **Partial indexes**: `WHERE banned = TRUE`, `WHERE peer_id != ''` for performance
- **Auto-detection**: DSN starting with `postgres://` or `postgresql://` automatically selects PostgreSQL

#### Schema Overview

```sql
-- Core tables (both backends)
peers              -- Device records (21 fields: id, uuid, pk, ip, note, status, etc.)
server_config      -- Key-value configuration store
id_change_history  -- Device ID change audit trail
users              -- Admin/operator/viewer accounts (PBKDF2 + TOTP)
api_keys           -- API key management (SHA256 hash storage)
device_tokens      -- Enrollment tokens (Dual Key System)
```

### TLS Support

Yomie supports TLS on all transport layers with a unique **dual-mode auto-detection** system:

```bash
# Enable TLS on signal ports (21116 TCP + 21115 + 21118 WSS)
./yomie-server -tls-signal -tls-cert server.crt -tls-key server.key

# Enable TLS on relay ports (21117 TCP + 21119 WSS)
./yomie-server -tls-relay -tls-cert server.crt -tls-key server.key

# Enable TLS on everything
./yomie-server -tls-signal -tls-relay -tls-cert server.crt -tls-key server.key

# HTTPS on API
./yomie-server -tls-cert server.crt -tls-key server.key -force-https
```

#### DualModeListener (config/tls.go)

Accepts **both plain and TLS connections on the same port**:

1. Peeks first byte of incoming connection
2. If `0x16` (TLS ClientHello) → upgrades to `tls.Server()`
3. Otherwise → passes through as plain TCP via `peekedConn`

This means existing RustDesk clients (no TLS) continue to work alongside TLS-enabled clients without port changes. Minimum TLS version: **TLS 1.2**.

### Rate Limiting & DDoS Protection

| Layer | Type | Default | Configurable |
|-------|------|---------|-------------|
| **IP rate limiter** | Sliding window per-IP | 20 registrations/min | Via constants |
| **Login rate limiter** | Per-IP | 5 attempts / 5 min | Via constants |
| **Bandwidth limiter** | Token bucket | 1 GB/s global, 16 MB/s per-session | Via constants |
| **Connection limiter** | Per-IP concurrent | 20 relay connections/IP | `-relay-max-conns-ip` |
| **TCP punch cache** | TTL + max size | 2 min TTL, 10K max entries | Via constants |
| **WebSocket origins** | Whitelist | Accept all | `WS_ALLOWED_ORIGINS` env |
| **Relay idle timeout** | Per-session | 30s (extended on activity) | Via constants |
| **Relay pair timeout** | Pending sessions | 30s | Via constants |

### Enrollment & Device Tokens

The **Dual Key System** controls which devices can register with the server:

| Mode | Behavior |
|------|----------|
| `open` (default) | Accept all device registrations |
| `managed` | New devices need admin approval OR a valid enrollment token |
| `locked` | Only devices with pre-issued valid enrollment tokens can register |

```bash
# Set enrollment mode
./yomie-server -mode all
# Then via API: PUT /api/enrollment/mode {"mode": "managed"}
```

Device tokens have statuses: `pending`, `active`, `revoked`, `expired`. Bulk generation is supported via `POST /api/tokens/generate-bulk`.

### 4-Tier Device Status System

```
ONLINE    → Last heartbeat < 15s ago
DEGRADED  → 2 consecutive missed heartbeats (15-30s)
CRITICAL  → 4+ missed heartbeats (30-60s)
OFFLINE   → Beyond RegTimeout (30s) with no heartbeat
```

- **Heartbeat interval**: Clients send `RegisterPeer` every ~12s
- **Cleaner goroutine**: Runs every 3s, checks all peers, publishes status transition events
- **Debounced DB sync**: Memory → database sync every 60s (avoids write storms)

### Admin TCP Console

A lightweight management interface accessible via `telnet` or `netcat`:

```bash
./yomie-server -admin-port 9090 -admin-password "secret"
# Then: telnet 127.0.0.1 9090
```

**Commands**: `status`, `peers [count|info <id>]`, `ban/unban/kick <id>`, `blocklist [add|rm]`, `config [get|set]`, `reload`, `quit`

> Admin console binds to `127.0.0.1` only — never exposed to the network.

---

## 🖥️ Web Console (Node.js)

The web console (`web-nodejs/`) is an Express.js application providing a full-featured management UI.

### Features

- **Dashboard** — Real-time statistics cards (total, active, inactive, banned devices)
- **Device management** — Responsive devices page with horizontal folder chips, unified toolbar with segmented filters, slim table, and kebab context menu (⋮). Four breakpoints: desktop, tablet (≤768px), phone card layout (≤600px), small phone (≤400px)
- **Device details** — Hardware tab (sysinfo), metrics tab (live CPU/RAM/disk bars + history charts)
- **TOTP 2FA** — Two-factor authentication with `otplib`
- **RBAC** — Admin, Operator, Viewer, and Pro (API-only) roles with permission enforcement
- **Address book sync** — Full AB storage with `address_books` table
- **RustDesk Client API** — Dedicated WAN port (21121) with 7-layer security
- **Desktop connect** — One-click connect via `rustdesk://` URI handler
- **i18n** — JSON-based translations, 25+ languages (auto-discovery from `lang/` directory)
- **Desktop widget dashboard** — Drag-and-drop dashboard mode with 20+ widget types (weather, calendar, system monitor, disk usage, log viewer, alert feed, speed test, Docker containers, world clock, bookmarks, device map, and more), Windows 11-style snap layouts with edge snapping, draggable zone borders, Aero Shake, widget groups/stacking, glassmorphism theme, presets/templates
- **Fleet management** — Device groups, tags, batch operations, scaling policies
- **Chat 2.0** — Operator↔device messaging with E2E encryption (ECDH P-256 + AES-256-GCM), read receipts, file sharing, typing indicators
- **Web remote client** — Browser-based remote desktop (H.264 via WebCodecs on HTTPS, JMuxer fallback on HTTP), session recording (WebM VP9+Opus), monitor switching, quality presets (Speed/Balanced/Quality/Best)
- **File transfer** — Browser-based file transfer with progress tracking
- **Security audit** — Built-in security scanner with compliance reporting
- **CDAP panel** — Connected Device Automation Protocol widget rendering + commands
- **Resource control** — USB, optical drive, monitor, disk, quota policy management
- **Unattended access** — Access policies with day/time schedules, operator restrictions, device passwords
- **Wake-on-LAN** — Remote power-on via magic packet (UDP broadcast)
- **Pro-only accounts** — API-only user role that activates RustDesk Pro features without web panel access
- **OS-style login screen** — Windows 11-style login page with frosted glass, clock overlay, multi-user selector, TOTP flow
- **Toast notifications** — Slide-in notifications with progress bar auto-dismiss
- **Light/dark/auto theme** — Full theme system with `prefers-color-scheme` auto-detection
- **Skeleton loading** — Shimmer pulse placeholders during data loading
- **CSRF protection** — Double-submit cookie pattern with `csrf-csrf`
- **WebSocket** — Real-time status updates from Go server event bus

### RustDesk Client API (Port 21121)

Allows RustDesk desktop clients to:
- Login/logout with username and password
- Sync address books across devices
- Send heartbeats and system information
- Query device groups and user information

**Security middleware stack (7 layers)**:

| Layer | Protection |
|-------|-----------|
| 1 | Request timeout (10s request / 15s headers) |
| 2 | Security headers (no server fingerprinting) |
| 3 | Request logger (full audit trail) |
| 4 | Path whitelist (only known RustDesk endpoints) |
| 5 | Global rate limit (100 req/15min per IP) |
| 6 | Login rate limit (5 attempts/15min per IP) |
| 7 | Body size limit (1KB default, 64KB for address book) |

---

## 🛡️ Security Architecture

### Server-Level Security

| Component | Implementation |
|-----------|---------------|
| **Transport encryption** | NaCl secretbox (signal TCP), TLS 1.2+ (all ports), WSS (WebSocket) |
| **Password storage** | PBKDF2-HMAC-SHA256, 100K iterations, 16-byte random salt |
| **JWT tokens** | HS256 with HMAC-equal verification (constant-time), configurable expiry, unique JTI |
| **2FA partial tokens** | 5-minute TTL (prevents long-lived intermediate auth states) |
| **TOTP 2FA** | RFC 6238 compliant, ±1 time step tolerance, constant-time code comparison |
| **API authentication** | JWT Bearer token OR API key (`X-API-Key` header, SHA256 hash lookup) |
| **Input validation** | Peer ID: `^[A-Za-z0-9_-]{6,16}$`, config key: `^[A-Za-z0-9_.\-]{1,64}$` |
| **SQL injection** | Parameterized queries only, LIKE patterns escape `%` and `_` |
| **IP blocklist** | IP, CIDR, device ID blocking with hot-reload (SIGHUP) |
| **Rate limiting** | Per-IP sliding window (registrations, login, TCP connections) |
| **Bandwidth limiting** | Token bucket (global: 1 GB/s, per-session: 16 MB/s) |
| **Audit logging** | Ring buffer (10K events) + optional JSON-lines file output |
| **Error handling** | Never exposes internal details to clients |
| **Credentials file** | Admin password file written with mode `0600` |
| **Proxy trust** | `X-Forwarded-For` only when `TRUST_PROXY=Y` |

### Console-Level Security

| Component | Implementation |
|-----------|---------------|
| **CSRF** | Double-submit cookie pattern (`csrf-csrf`) |
| **Session fixation** | Session regeneration after login |
| **Timing-safe auth** | Pre-computed dummy bcrypt hash for non-existent users |
| **WebSocket auth** | Session cookie required for WS upgrade |
| **Helmet** | Security headers (CSP, HSTS, X-Frame-Options) |
| **Rate limiting** | `express-rate-limit` on all endpoints |
| **XSS prevention** | HTML sanitization, `textContent` instead of `innerHTML` |
| **Sort injection** | Whitelisted sort fields and directions |

---

## �️ Platform Support

| Platform | Tier | Status | Notes |
|----------|------|--------|-------|
| **Linux x86_64** (bare-metal) | Tier 1 | ✅ Primary | Full support, all features, recommended |
| **Linux ARM64** (bare-metal) | Tier 2 | ✅ Supported | Raspberry Pi 4+, Oracle ARM |
| **Docker** (single-container) | Tier 2 | ✅ Supported | All-in-one image with supervisord |
| **PostgreSQL** backend | Tier 2 | ✅ Supported | Enterprise deployments |
| **Windows** x86_64 | Tier 3 | ⚠️ Experimental | Community-tested, limited support |
| **Synology DSM** (Docker) | Tier 3 | ⚠️ Experimental | Community-tested |

> **Tier 1** = Fully tested by maintainers, highest priority for bug fixes.
> **Tier 2** = Supported and tested, fixes provided.
> **Tier 3** = Community-tested, best-effort support. Contributions welcome!

---

## �📦 Installation

### Prerequisites

| Requirement | Version | Notes |
|------------|---------|-------|
| **Go** | 1.21+ | Auto-installed by scripts if missing (downloads Go 1.22.1) |
| **Node.js** | 18+ | Auto-installed by scripts if missing |
| **Git** | Any | For cloning the repository |
| **OS** | Linux (Ubuntu 20.04+, Debian 11+, CentOS 8+), Windows 10+, Docker | |
| **PostgreSQL** | 14+ | Optional — SQLite is the default |

### Linux

```bash
git clone https://github.com/shamstabraiz/Rustdesk-FreeConsole.git
cd Rustdesk-FreeConsole
chmod +x yomie.sh

# Interactive mode (recommended for first install)
sudo ./yomie.sh

# Automatic mode (non-interactive)
sudo ./yomie.sh --auto

# Skip Go binary SHA256 verification
sudo ./yomie.sh --skip-verify

# Custom API port
API_PORT=21120 sudo ./yomie.sh --auto
```

The script will:
1. Install Go toolchain if not present
2. Compile `yomie-server` from source (single binary)
3. Install Node.js and the web console
4. Create systemd services (`yomie-server.service` + `yomie-console.service`)
5. Generate Ed25519 keys (or preserve existing ones)
6. Create initial admin user (credentials saved to `.admin_credentials`)
7. Start all services

### Windows

```powershell
# Run PowerShell as Administrator
git clone https://github.com/shamstabraiz/Rustdesk-FreeConsole.git
cd Rustdesk-FreeConsole

# Interactive mode
.etterdesk.ps1

# Automatic mode
.etterdesk.ps1 -Auto

# Skip verification
.etterdesk.ps1 -SkipVerify
```

The script installs Go, compiles the server, sets up NSSM services (`YomieServer` + `YomieConsole`) or scheduled tasks as fallback.

### Docker

**🚀 Quick Start (no build required):**

```bash
# Download and run - that's it!
curl -fsSL https://raw.githubusercontent.com/shamstabraiz/Rustdesk-FreeConsole/main/docker-compose.quick.yml -o docker-compose.yml
docker compose up -d

# Get admin password
docker compose logs console 2>&1 | grep -i "Admin password"
```

Open http://localhost:5000 — done in 30 seconds! See [DOCKER_QUICKSTART.md](docs/docker/DOCKER_QUICKSTART.md) for more options.

**Build from source (advanced):**

```bash
git clone https://github.com/shamstabraiz/Rustdesk-FreeConsole.git
cd Rustdesk-FreeConsole

# Build and start (all-in-one: Go server + Node.js console in one container)
docker compose -f docker-compose.single.yml build
docker compose -f docker-compose.single.yml up -d
```

**With PostgreSQL:**

```bash
docker compose -f docker-compose.single.yml --profile postgres up -d
```

**Legacy multi-container** (via interactive script):

```bash
chmod +x yomie-docker.sh
./yomie-docker.sh
```

### Menu Options

All scripts provide an interactive menu:

| Option | Description |
|--------|-------------|
| **1** | 🚀 New installation (compile Go server + install Node.js console) |
| **2** | ⬆️ Update existing installation |
| **3** | 🔧 Repair installation |
| **4** | ✅ Validate installation |
| **5** | 💾 Create backup |
| **6** | 🔐 Reset admin password |
| **7** | 🔨 Build/rebuild Go server binary |
| **8** | 📊 Diagnostics |
| **9** | 🗑️ Uninstall |
| **C** | 🔒 Configure SSL/TLS certificates |
| **M** | 🔄 Migrate from existing RustDesk (Docker) |
| **P** | 🔀 Database migration (SQLite ↔ PostgreSQL) |
| **S** | ⚙️ Settings (path configuration) |

### What Gets Installed

```
/opt/rustdesk/                    # (Linux) or C:\Yomie\ (Windows)
├── yomie-server             # Single Go binary (signal + relay + API)
├── id_ed25519                    # Ed25519 private key (mode 0600)
├── id_ed25519.pub                # Ed25519 public key (base64)
├── db_v2.sqlite3                 # SQLite database (if using SQLite)
├── .admin_credentials            # Initial admin password (mode 0600)
├── .api_key                      # API key for console ↔ server auth
└── web-console/                  # Node.js web console
    ├── server.js
    ├── package.json
    ├── .env                      # Console configuration
    └── ...
```

---

## 🖥️ RustDesk Client Configuration

After installing Yomie server, configure your RustDesk desktop clients to connect to it.

### Basic Setup

1. Open the RustDesk client
2. Click the **menu (≡)** button → **Network** → **ID/Relay Server**
3. Fill in:

| Field | Value | Example |
|-------|-------|---------|
| **ID Server** | Your server IP or domain | `yomie.example.com` |
| **Relay Server** | Same as ID Server (or leave empty) | `yomie.example.com` |
| **API Server** | `http(s)://<server>:21121` | `http://yomie.example.com:21121` |
| **Key** | Contents of `id_ed25519.pub` on the server | (base64 public key string) |

> **Tip:** The public key can be found in the Web Console under **Dashboard → Server Keys**, or by reading the file `/opt/rustdesk/id_ed25519.pub` (Linux) / `C:\Yomie\id_ed25519.pub` (Windows) on the server.

### Mass Deployment

For deploying to many clients, use the RustDesk configuration string:

```
rustdesk://config/<base64-encoded-json>
```

JSON structure:
```json
{
  "host": "yomie.example.com",
  "relay": "yomie.example.com",
  "api": "http://yomie.example.com:21121",
  "key": "<contents-of-id_ed25519.pub>"
}
```

You can also use the `--config` command-line flag when starting RustDesk:
```bash
rustdesk --config '{"host":"yomie.example.com","key":"<pubkey>"}'
```

### Desktop Client Login

RustDesk desktop clients can **log in** to the Yomie server using their user account. This enables address book synchronization, device grouping, and audit trail per user.

#### How to Log In

1. Open the RustDesk client
2. Click the **account icon** (person silhouette) in the bottom-left corner
3. Enter your **username** and **password** (the same credentials used for the Web Console)
4. Click **Login**

> **Note:** The login API runs on port **21121** (the RustDesk Client API), not on port 21114 (which is the server management REST API for LAN-only use). Make sure the `API Server` field in the client points to `http(s)://<server>:21121`.

#### What Login Enables

| Feature | Description |
|---------|-------------|
| **Address Book Sync** | Your address book is stored server-side and synced across all devices where you're logged in |
| **User Identity** | Connections are associated with your user account in audit logs |
| **Device Groups** | Access to pre-configured device groups and strategies |
| **Heartbeat + Sysinfo** | The client periodically sends system information (CPU, RAM, OS) to the server |

#### User Roles and API Access

| Role | Web Panel | RustDesk Client Login | Address Book | Device Groups | Device List |
|------|:---------:|:--------------------:|:------------:|:-------------:|:-----------:|
| **Admin** | ✅ Full access | ✅ | ✅ | ✅ | ✅ |
| **Operator** | ✅ Device management | ✅ | ✅ | ✅ | ✅ |
| **Viewer** | ✅ Read-only | ✅ | ✅ | ✅ | ✅ |
| **Pro** | ❌ Blocked | ✅ | ✅ | ❌ Empty | ❌ Empty |

> **Pro accounts** are designed for end-users who need RustDesk Pro features (address book sync, heartbeat, sysinfo) without access to the web management panel or visibility into other devices/groups. Create Pro users via the Web Console → Users page.

#### Troubleshooting Login

| Problem | Solution |
|---------|----------|
| "Login failed" | Verify the API Server field is set to `http://<server>:21121` (not 21114) |
| "Connection refused" | Ensure port 21121 is open in the firewall and the console is running |
| TOTP 2FA prompt | Enter the 6-digit code from your authenticator app (if 2FA is enabled for your account) |
| "Invalid credentials" | Reset password via Web Console → Users, or run the installer with option 6 |

### Enabling Pro Features

Connecting the RustDesk desktop client to a Yomie server with the API Server field configured **automatically activates Pro-level features** — no license key required. These features are built into the standard RustDesk client but remain dormant until a compatible API server is detected.

#### How to Activate

1. Open the RustDesk client
2. Go to **Settings (⚙) → Network → ID/Relay Server** (or **≡ → Network**)
3. Fill in all four fields:

| Field | Value |
|-------|-------|
| **ID Server** | `yomie.example.com` |
| **Relay Server** | `yomie.example.com` (or leave empty to auto-detect) |
| **API Server** | `http://yomie.example.com:21121` |
| **Key** | Contents of `id_ed25519.pub` from the server |

4. Click the **account icon** (bottom-left) and **log in** with your Yomie credentials
5. Pro features activate immediately upon successful login

> **Important:** The **API Server** field is the key trigger. Without it, the client operates in basic mode. The field must point to port **21121** (the RustDesk Client API), not to port 21114 (server management API).

#### Features Enabled After Login

| Feature | Description | Without API | With API + Login |
|---------|-------------|:-----------:|:----------------:|
| **Address Book Sync** | Address book stored server-side and synced across all logged-in devices | Local only | ✅ Cloud sync |
| **Device Groups** | Organize devices into groups with access control strategies | — | ✅ |
| **User Groups** | Group-based access policies and permissions | — | ✅ |
| **Audit Trail** | Connection events, file transfers, and alarms logged per user | — | ✅ |
| **Heartbeat + Sysinfo** | Client reports CPU, RAM, OS, hostname periodically to the server | — | ✅ |
| **Device Metrics** | Live CPU/RAM/disk usage visible in the Web Console device panel | — | ✅ |
| **Connection Audit** | Each remote session logged with start/end time, peer IDs, user identity | — | ✅ |
| **File Transfer Audit** | Every file sent/received is recorded in the audit log | — | ✅ |
| **Security Alarms** | Brute-force attempts, unauthorized access, and suspicious activity alerts | — | ✅ |
| **Access Strategies** | Admin-defined rules controlling who can connect to which devices | — | ✅ |
| **Multi-Device Login** | Same account logged in on multiple machines, all sharing one address book | — | ✅ |
| **TOTP 2FA** | Two-factor authentication prompt when logging in (if enabled for the account) | — | ✅ |

#### Verifying Pro Mode Is Active

After logging in, you can confirm Pro features are working:

- **Address book** — Open the address book panel; entries sync from the server. Adding a device on one machine appears on others.
- **Account icon** — The bottom-left icon shows your username instead of a generic silhouette.
- **Web Console** — Navigate to **Devices** in the Web Console; your client should appear with live status, sysinfo (Hardware tab), and real-time metrics (Metrics tab).

#### Mass Deployment with Pro Enabled

To deploy pre-configured clients with Pro features active across your organization:

```bash
# Configuration string (Base64-encoded JSON)
rustdesk://config/eyJob3N0IjoiYmV0dGVyZGVzay5leGFtcGxlLmNvbSIsInJlbGF5IjoiYmV0dGVyZGVzay5leGFtcGxlLmNvbSIsImFwaSI6Imh0dHA6Ly9iZXR0ZXJkZXNrLmV4YW1wbGUuY29tOjIxMTIxIiwia2V5IjoiPHB1YmtleT4ifQ==
```

Or via command line:
```bash
rustdesk --config '{"host":"yomie.example.com","relay":"yomie.example.com","api":"http://yomie.example.com:21121","key":"<pubkey>"}'
```

> **Note:** Users still need to log in individually after initial configuration to activate per-user features (address book sync, audit trail, etc.).

### Ports Required on Client Side

Ensure the following **outbound** ports are accessible from clients to the server:

| Port | Protocol | Purpose |
|------|----------|---------|
| 21115 | TCP | NAT type detection (automatic) |
| 21116 | TCP + UDP | Signal / ID registration |
| 21117 | TCP | Relay (connection forwarding) |
| 21118 | TCP (WS) | WebSocket signal (optional, web clients) |
| 21119 | TCP (WS) | WebSocket relay (optional, web clients) |
| 21121 | TCP | RustDesk Client API — login, address book sync, heartbeat |

> **Port 21114** is the Go server management REST API (LAN-only) — it is **not** used by RustDesk desktop clients. Do not expose it to the internet.

---

## 🔒 TLS / SSL Certificates

Yomie supports TLS on all layers: Go server transport (signal + relay), Go server HTTPS API, and the Node.js web console.

### Self-Signed Certificate (Quick Start)

For testing or internal networks, generate a self-signed certificate:

```bash
# Create certificate directory
mkdir -p /opt/rustdesk/ssl

# Generate a self-signed certificate (valid for 3 years)
openssl req -x509 -nodes -days 1095 -newkey rsa:2048 \
  -keyout /opt/rustdesk/ssl/yomie.key \
  -out /opt/rustdesk/ssl/yomie.crt \
  -subj "/CN=$(hostname -f)/O=Yomie/C=US" \
  -addext "subjectAltName=DNS:$(hostname -f),DNS:localhost,IP:$(curl -s ifconfig.me),IP:127.0.0.1"

# Secure the private key
chmod 600 /opt/rustdesk/ssl/yomie.key
```

> **Windows (PowerShell)**:
> ```powershell
> New-Item -ItemType Directory -Path "C:\Yomie\ssl" -Force
> openssl req -x509 -nodes -days 1095 -newkey rsa:2048 `
>   -keyout "C:\Yomie\ssl\yomie.key" `
>   -out "C:\Yomie\ssl\yomie.crt" `
>   -subj "/CN=localhost/O=Yomie"
> ```

### Applying TLS to Go Server

Once you have certificate files, configure the Go server:

```bash
# TLS on signal ports (21116 TCP + 21115 + 21118 WSS)
./yomie-server -tls-signal -tls-cert /opt/rustdesk/ssl/yomie.crt -tls-key /opt/rustdesk/ssl/yomie.key

# TLS on relay ports (21117 TCP + 21119 WSS)
./yomie-server -tls-relay -tls-cert /opt/rustdesk/ssl/yomie.crt -tls-key /opt/rustdesk/ssl/yomie.key

# TLS everywhere + force HTTPS on API
./yomie-server -tls-signal -tls-relay -force-https \
  -tls-cert /opt/rustdesk/ssl/yomie.crt \
  -tls-key /opt/rustdesk/ssl/yomie.key
```

For systemd, add the flags to `ExecStart` in `/etc/systemd/system/yomie-server.service`:
```ini
ExecStart=/opt/rustdesk/yomie-server -mode all ... \
  -tls-signal -tls-relay \
  -tls-cert /opt/rustdesk/ssl/yomie.crt \
  -tls-key /opt/rustdesk/ssl/yomie.key
```

> **Dual-mode**: Yomie auto-detects plain vs TLS on the **same port** (first-byte `0x16` detection). Existing non-TLS clients continue to work without changes.

### Applying TLS to Web Console (Node.js)

Edit the console `.env` file:

```bash
HTTPS_ENABLED=true
SSL_CERT_PATH=/opt/rustdesk/ssl/yomie.crt
SSL_KEY_PATH=/opt/rustdesk/ssl/yomie.key
HTTP_REDIRECT_HTTPS=true
```

Or use the ALL-IN-ONE script menu option **C** (SSL Configuration) which supports:
- **Let's Encrypt** — automatic certificate via certbot (requires public domain + port 80)
- **Custom certificate** — provide your own PEM cert + key
- **Self-signed** — auto-generated for testing

### Let's Encrypt (Production)

For production servers with a public domain:

```bash
# Install certbot
sudo apt install certbot  # Debian/Ubuntu

# Obtain certificate (standalone mode, port 80 must be open)
sudo certbot certonly --standalone -d yomie.example.com

# Certificate files:
# /etc/letsencrypt/live/yomie.example.com/fullchain.pem
# /etc/letsencrypt/live/yomie.example.com/privkey.pem
```

Then configure both the Go server and Node.js console to use these paths.

> **Auto-renewal**: The ALL-IN-ONE script automatically adds a certbot renewal cron job when you use the Let's Encrypt option.

### Automatic TLS During Installation

The ALL-IN-ONE scripts (`yomie.sh` / `yomie.ps1`) **automatically generate a self-signed certificate** during every fresh installation:

1. **Certificate location**: `<RUSTDESK_PATH>/ssl/yomie.crt` + `yomie.key`
2. **Validity**: 3 years, RSA 2048-bit, with SAN (server IP + localhost)
3. **Go server flags**: `-tls-cert`, `-tls-key`, `-tls-signal`, `-tls-relay`, `-force-https` added automatically
4. **Node.js console**: `.env` pre-populated with `SSL_CERT_PATH` / `SSL_KEY_PATH` and `BETTERDESK_API_URL` set to `https://`
5. **Dual-mode**: Existing non-TLS clients continue to work (auto-detection on same port)

You can **upgrade to Let's Encrypt** or a custom certificate at any time using menu option **C**. If certificate generation fails during install (e.g., openssl missing), the server runs without TLS and you can generate certificates later.

---

## ⚙️ Configuration Reference

### CLI Flags

| Flag | Default | Env Var | Description |
|------|---------|---------|-------------|
| `-port` | `21116` | `PORT` | Signal port (UDP + TCP) |
| `-relay-port` | `21117` | `RELAY_PORT` | Relay port (TCP) |
| `-api-port` | `21114` | `API_PORT` | HTTP REST API port |
| `-mode` | `all` | `MODE` | Server mode: `all`, `signal`, `relay` |
| `-db` | `./db_v2.sqlite3` | `DB_URL` | Database DSN — file path (SQLite) or `postgres://...` (PostgreSQL) |
| `-key-file` | `id_ed25519` | `KEY_FILE` | Ed25519 key file path (without extension) |
| `-relay-servers` | *(empty)* | `RELAY_SERVERS` | Comma-separated relay addresses for `ConfigUpdate` |
| `-rendezvous-servers` | *(empty)* | `RENDEZVOUS_SERVERS` | Comma-separated rendezvous addresses |
| `-mask` | *(empty)* | `MASK` | LAN mask (e.g. `192.168.0.0/24`) |
| `-always-relay` | `false` | `ALWAYS_USE_RELAY=Y` | Force relay for all connections (no P2P) |
| `-blocklist` | *(empty)* | `BLOCKLIST_FILE` | Path to IP/ID/CIDR blocklist file |
| `-audit-log` | *(empty)* | `AUDIT_LOG_FILE` | Path to audit log file (JSON lines) |
| `-tls-cert` | *(empty)* | `TLS_CERT` | TLS certificate file (PEM) |
| `-tls-key` | *(empty)* | `TLS_KEY` | TLS private key file (PEM) |
| `-tls-signal` | `false` | `TLS_SIGNAL=Y` | Enable TLS on signal ports |
| `-tls-relay` | `false` | `TLS_RELAY=Y` | Enable TLS on relay ports |
| `-log-format` | `text` | `LOG_FORMAT` | Log format: `text` or `json` |
| `-admin-port` | `0` | `ADMIN_PORT` | TCP admin console port (0 = disabled) |
| `-admin-password` | *(empty)* | `ADMIN_PASSWORD` | Admin TCP console password |
| `-jwt-secret` | *(auto)* | `JWT_SECRET` | JWT signing secret (auto-generated if omitted) |
| `-jwt-expiry` | `24` | `JWT_EXPIRY_HOURS` | JWT token expiry in hours |
| `-force-https` | `false` | `FORCE_HTTPS=Y` | Reject non-TLS API requests |
| `-trust-proxy` | `false` | `TRUST_PROXY=Y` | Trust `X-Forwarded-For` / `X-Real-IP` headers |
| `-relay-max-conns-ip` | `20` | `RELAY_MAX_CONNS_PER_IP` | Max relay connections per IP |
| `-init-admin-user` | `admin` | `INIT_ADMIN_USER` | Initial admin username |
| `-init-admin-pass` | *(auto)* | `INIT_ADMIN_PASS` | Initial admin password (auto-generated if omitted) |
| `-version` | — | — | Show version and exit |

### Environment-Only Variables

| Env Var | Description |
|---------|-------------|
| `WS_ALLOWED_ORIGINS` | Comma-separated allowed WebSocket origins |
| `ENROLLMENT_MODE` | Device enrollment: `open`, `managed`, or `locked` |

### Protocol Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `RegTimeout` | 30s | Time after last heartbeat to mark peer offline |
| `HeartbeatCheck` | 3s | Status cleaner goroutine interval |
| `HeartbeatExpected` | 15s | Expected heartbeat interval from clients |
| `DegradedThreshold` | 2 missed | Transitions to DEGRADED status |
| `CriticalThreshold` | 4 missed | Transitions to CRITICAL status |
| `HeartbeatSuggestion` | 12s | Suggested heartbeat interval sent to clients |
| `TCPConnTimeout` | 20s | TCP connection establishment timeout |
| `RelayPairTimeout` | 30s | Time to wait for relay pair match |
| `RelayIdleTimeout` | 30s | Close idle relay sessions (extended on activity) |
| `DefaultTotalBandwidth` | 1 GB/s | Global bandwidth limit |
| `DefaultSingleBandwidth` | 16 MB/s | Per-session bandwidth limit |
| `IPRateLimitRegistrations` | 20/min | Registration rate limit per IP |
| `IDChangeCooldown` | 5 min | Minimum interval between ID changes |
| `MaxFrameSize` | 64 KB | Maximum wire protocol frame size |

### Console Configuration (.env)

```bash
PORT=5000              # Web console port
HOST=0.0.0.0           # Web console bind address (LAN)
API_PORT=21121         # RustDesk Client API port
API_HOST=0.0.0.0       # RustDesk Client API bind address
API_ENABLED=true       # Enable/disable Client API
HTTPS_ENABLED=false    # Enable HTTPS on console
SSL_CERT_PATH=         # SSL certificate path
SSL_KEY_PATH=          # SSL key path
TRUST_PROXY=false      # Trust X-Forwarded-For
DB_PATH=               # Path to SQLite database
BETTERDESK_API_URL=    # Go server API URL (http://localhost:21114)
BETTERDESK_API_KEY=    # API key for Go server (env: BETTERDESK_API_KEY or HBBS_API_KEY)
```

> Keep port `5000` LAN-only via firewall. Do not expose the Web Console directly to the internet.

---

## 📡 API Reference

### Authentication

All API requests (except public endpoints) require authentication:

```bash
# Option 1: JWT Bearer token (from POST /api/auth/login)
curl -H "Authorization: Bearer <jwt_token>" http://localhost:21114/api/peers

# Option 2: API key
curl -H "X-API-Key: <your_api_key>" http://localhost:21114/api/peers
```

### Public Endpoints (No Auth)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | Health check + peer counts |
| `GET` | `/api/server/stats` | Server stats (memory, goroutines, peer breakdown) |
| `GET` | `/api/server/pubkey` | Ed25519 public key (base64 + hex) |
| `GET` | `/metrics` | Prometheus metrics |
| `POST` | `/api/auth/login` | Login → JWT token |
| `POST` | `/api/auth/login/2fa` | Complete TOTP 2FA flow |

### RustDesk Client Endpoints (Go Server, Port 21114)

These endpoints are served on the Go server's management API port because the RustDesk desktop client calculates the API port as `signal_port - 2` (21116 - 2 = 21114):

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/login` | RustDesk-compatible login (username/password + optional TOTP) |
| `GET` | `/api/login-options` | Available authentication methods |
| `POST` | `/api/logout` | Invalidate session (no-op for stateless JWT) |
| `GET/POST` | `/api/currentUser` | Get current user info (Bearer token) |
| `GET/POST` | `/api/ab` | Get/update address book |
| `POST` | `/api/heartbeat` | Client heartbeat (updates peer status to ONLINE) |
| `POST` | `/api/sysinfo` | Client system info (hostname, OS, version) |
| `POST` | `/api/sysinfo_ver` | Sysinfo version check (SHA256 of stored fields) |

### Viewer+ Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/peers` | List all peers (`?include_deleted=true`) |
| `GET` | `/api/peers/{id}` | Get single peer |
| `GET` | `/api/peers/status/summary` | Aggregate status counts |
| `GET` | `/api/peers/online` | All online peer snapshots |
| `GET` | `/api/peers/{id}/status` | Detailed live status (missed beats, transport) |
| `GET` | `/api/blocklist` | List blocklist entries |
| `PUT` | `/api/peers/{id}/tags` | Set peer tags |
| `GET` | `/api/tags/{tag}/peers` | Peers by tag (LIKE pattern, `%_` escaped) |
| `GET` | `/api/audit/events` | Recent events (`?limit=50&action=peer_banned`) |
| `GET` | `/api/peers/{id}/metrics` | Historical CPU/RAM/disk metrics (`?limit=100`, max 1000) |
| `GET` | `/api/ws/events` | WebSocket real-time events (`?filter=peer_online`) |
| `GET` | `/api/auth/me` | Current user info |

### Admin-Only Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `DELETE` | `/api/peers/{id}` | Delete peer (`?hard=true` for permanent) |
| `POST` | `/api/peers/{id}/ban` | Ban peer (with reason) |
| `POST` | `/api/peers/{id}/unban` | Unban peer |
| `POST` | `/api/peers/{id}/change-id` | Change device ID (validated: 6-16 chars, `[A-Za-z0-9_-]`) |
| `POST` | `/api/blocklist` | Add IP/CIDR/ID to blocklist |
| `DELETE` | `/api/blocklist/{entry}` | Remove blocklist entry |
| `GET/PUT` | `/api/config/{key}` | Server configuration (key validated: 1-64 chars) |
| `GET/POST/PUT/DELETE` | `/api/users`, `/api/users/{id}` | User CRUD |
| `POST` | `/api/users/{id}/totp/setup` | Generate TOTP secret + QR URI |
| `POST` | `/api/users/{id}/totp/confirm` | Confirm TOTP enrollment |
| `DELETE` | `/api/users/{id}/totp` | Disable TOTP |
| `GET/POST/DELETE` | `/api/keys`, `/api/keys/{id}` | API key management |
| `GET/POST/PUT/DELETE` | `/api/tokens`, `/api/tokens/{id}` | Device token CRUD |
| `POST` | `/api/tokens/generate-bulk` | Bulk generate enrollment tokens |
| `POST` | `/api/tokens/{id}/bind` | Bind token to specific peer ID |
| `GET/PUT` | `/api/enrollment/mode` | Enrollment mode management |
| `POST` | `/api/peers/{id}/wol` | Wake-on-LAN magic packet (UDP broadcast) |
| `GET/POST/PUT/DELETE` | `/api/access-policies`, `/api/access-policies/{id}` | Unattended access policies (schedules, operator restrictions) |

### Example: Login Flow

```bash
# Step 1: Login (returns JWT or requires 2FA)
curl -X POST http://localhost:21114/api/auth/login   -H "Content-Type: application/json"   -d '{"username":"admin","password":"your_password"}'

# Response (no 2FA):
# {"token":"eyJhbG...","user":{"id":1,"username":"admin","role":"admin"}}

# Response (2FA required):
# {"requires_2fa":true,"partial_token":"eyJhbG..."}

# Step 2 (if 2FA): Complete with TOTP code
curl -X POST http://localhost:21114/api/auth/login/2fa   -H "Content-Type: application/json"   -d '{"partial_token":"eyJhbG...","code":"123456"}'
```

### WebSocket Events

```javascript
// Connect to real-time event stream
const ws = new WebSocket('ws://localhost:21114/api/ws/events?filter=peer_online,peer_offline');

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  // { "type": "peer_online", "peer_id": "ABC123", "timestamp": "..." }
};
```

**Event types**: `peer_online`, `peer_degraded`, `peer_critical`, `peer_offline`, `peer_banned`, `peer_unbanned`, `peer_deleted`, `peer_id_changed`, `blocklist_updated`, `server_stats`

---

## 🔄 Migration Guide

### From Original RustDesk (Rust) to Yomie (Go)

The migration tool (`yomie-server/tools/migrate/`) supports multiple migration paths:

**Linux/macOS:**
```bash
# Compile migration tool
cd yomie-server/tools/migrate
go build -o migrate .

# Mode 1: Rust hbbs → Yomie Go (preserves peer table → peers)
./migrate -mode rust2go -src /opt/rustdesk/db_v2.sqlite3 -dst /opt/yomie/db_v2.sqlite3

# Mode 2: SQLite → PostgreSQL
./migrate -mode sqlite2pg -src /opt/yomie/db_v2.sqlite3   -dst "postgres://user:pass@localhost:5432/yomie"

# Mode 3: PostgreSQL → SQLite (reverse)
./migrate -mode pg2sqlite -src "postgres://..." -dst ./backup.sqlite3

# Mode 4: Node.js console → Go server
./migrate -mode nodejs2go -src /opt/yomie/web-console/yomie.db   -dst /opt/yomie/db_v2.sqlite3

# Mode 5: Backup
./migrate -mode backup -src /opt/yomie/db_v2.sqlite3
```

**Windows (PowerShell):**
```powershell
# Compile migration tool (requires Go installed)
cd yomie-server\tools\migrate
go build -o migrate.exe .

# Usage (same modes as Linux)
.\migrate.exe -mode rust2go -src C:\Yomie\db_v2.sqlite3 -dst C:\Yomie\db_v2_new.sqlite3
.\migrate.exe -mode sqlite2pg -src C:\Yomie\db_v2.sqlite3 -dst "postgres://user:pass@localhost:5432/yomie"
```

> **Note:** Windows users need [Go](https://go.dev/dl/) installed to compile the migration tool. Pre-built binaries are available in [GitHub Releases](https://github.com/shamstabraiz/Yomie/releases) (when available).

The migration tool auto-detects the source schema (original RustDesk `peer` table vs Yomie `peers` table) and maps columns accordingly. Ed25519 keys, UUIDs, ID history, bans, and tags are fully preserved.

### Using ALL-IN-ONE Scripts

Both `yomie.sh` and `yomie.ps1` include built-in migration options:
- **Option M** — Migrate from existing RustDesk Docker installation
- **Option P** — Database migration (SQLite ↔ PostgreSQL)

### From Existing Docker RustDesk

```bash
./yomie-docker.sh
# Select: M (Migrate from existing RustDesk)
```

The wizard auto-detects existing containers, creates a backup, and migrates data to Yomie.

---

## 📊 Monitoring & Metrics

### Prometheus Metrics

Available at `GET /metrics` (no authentication required):

```
# Counters (monotonic)
yomie_registrations_total
yomie_expired_total
yomie_relay_sessions_total
yomie_relay_bytes_total
yomie_bandwidth_throttle_hits_total
yomie_audit_events_total

# Gauges (current values)
yomie_uptime_seconds
yomie_peers_total
yomie_peers_online
yomie_peers_degraded
yomie_peers_critical
yomie_peers_offline
yomie_peers_banned
yomie_peers_udp
yomie_peers_tcp
yomie_peers_ws
yomie_relay_active_sessions
yomie_blocklist_entries
yomie_event_subscribers
yomie_goroutines
yomie_memory_alloc_bytes
yomie_memory_sys_bytes
```

### Grafana Integration

Point a Prometheus scraper at `http://your-server:21114/metrics` and import the metrics above. No additional exporter needed.

### Audit Logging

```bash
# Enable file-based audit log
./yomie-server -audit-log /var/log/yomie/audit.jsonl
```

Events logged: login, failed auth, peer banned/unbanned, config changes, ID changes, blocklist modifications. Each event includes timestamp, action, actor, target, IP, and details.

---

## 🔧 Troubleshooting

### Common Issues

| Symptom | Cause | Solution |
|---------|-------|----------|
| **Key mismatch** errors | Keys changed during install | Restore from backup: `cp /opt/rustdesk-backup-*/id_ed25519* /opt/rustdesk/` |
| **All devices offline** | Database missing `last_online` column | Run repair: `sudo ./yomie.sh` → option 3 |
| **API not responding** | Wrong binary or port | Check: `curl http://localhost:21114/api/health` |
| **E2E encryption** | Not showing green lock | Verify server is v2.4.0+; enable `--tls-relay` for extra layer |
| **Docker: "pull access denied"** | Images not on Docker Hub | Build locally: `docker compose build` |
| **Docker: "no such table"** | Wrong binary in container | Rebuild: `docker compose build --no-cache` |
| **Services won't start** | Permission issues | `sudo chmod 600 /opt/rustdesk/id_ed25519` |
| **Go compilation fails** | Missing Go toolchain | Script auto-installs, or: `sudo apt install golang-go` |
| **PostgreSQL connection** | Wrong DSN format | Must start with `postgres://` or `postgresql://` |
| **Connect button** won't work | Custom RustDesk scheme | Set via browser console: `setCustomScheme('your-scheme')` |

### Diagnostics

```bash
# Linux — run built-in diagnostics
sudo ./yomie.sh  # Select option 8

# Check service status
sudo systemctl status yomie-server
sudo systemctl status yomie-console

# Check logs
sudo journalctl -u yomie-server -n 100 --no-pager
sudo journalctl -u yomie-console -n 100 --no-pager

# Test API
curl http://localhost:21114/api/health
curl http://localhost:21114/api/server/stats

# Check metrics
curl http://localhost:21114/metrics

# Windows — check services
Get-Service YomieServer
Get-Service YomieConsole
```

### Key Management

```bash
# Backup keys (CRITICAL — losing keys disconnects ALL clients)
cp /opt/rustdesk/id_ed25519 /opt/rustdesk/id_ed25519.backup
cp /opt/rustdesk/id_ed25519.pub /opt/rustdesk/id_ed25519.pub.backup

# Restore keys from automatic backup
BACKUP=$(ls -d /opt/rustdesk-backup-* | sort | tail -1)
sudo cp $BACKUP/id_ed25519* /opt/rustdesk/
sudo chmod 600 /opt/rustdesk/id_ed25519
sudo systemctl restart yomie-server
```

### Firewall Configuration

```bash
# RustDesk protocol ports (WAN)
sudo ufw allow 21115/tcp    # NAT test
sudo ufw allow 21116/tcp    # Signal (TCP)
sudo ufw allow 21116/udp    # Signal (UDP)
sudo ufw allow 21117/tcp    # Relay
sudo ufw allow 21121/tcp    # RustDesk Client API (login/AB)

# Admin ports (LAN only)
sudo ufw allow from 192.168.0.0/16 to any port 21114 proto tcp  # REST API
sudo ufw allow from 192.168.0.0/16 to any port 5000 proto tcp   # Web Console
```

---

## 🔐 E2E Encryption

### Overview

RustDesk clients support end-to-end encryption for remote desktop sessions. Yomie Server fully supports this — both P2P (hole-punch) and relay-mode connections establish an encrypted E2E channel with NaCl key exchange.

### How It Works

1. Client A requests connection to Client B via the signal server
2. Signal server facilitates hole-punching (sends `PunchHoleResponse` with Client B's public key) or relay setup (sends `RelayResponse` with relay UUID and `SignIdPk` credential)
3. Once connected (P2P or relay), clients perform an **asymmetric key exchange** using NaCl (`Message.SignedId` + `Message.PublicKey`)
4. All subsequent session traffic (video, audio, clipboard, files) is encrypted end-to-end

### Key Implementation Details

| Component | Implementation |
|-----------|---------------|
| **PunchHoleResponse** | Includes `pk` field (responder's public key) for E2E handshake initiation |
| **RelayResponse** | Uses `SignIdPk()` NaCl combined format (64-byte Ed25519 signature + IdPk protobuf) |
| **Relay server** | Pure opaque byte pipe — does **not** send its own `RelayResponse` confirmation, allowing clients to exchange `Message.SignedId` / `Message.PublicKey` directly |
| **Transport layer** | Optional TLS via `--tls-relay` for defense-in-depth on top of E2E |

### Verified Behavior

- Green lock indicator appears in the RustDesk client for all connection modes
- `Message.SignedId` + `Message.PublicKey` handshake confirmed between peers via debug logging
- Compatible with RustDesk clients v1.1.9+ (standard and custom builds)

---

## � Chat E2E Encryption

Yomie includes a built-in chat system between operators and end-user devices with full end-to-end encryption.

### Protocol

| Component | Implementation |
|-----------|---------------|
| **Key exchange** | P-256 ECDH via WebCrypto API |
| **Key derivation** | HKDF-SHA256 |
| **Message encryption** | AES-256-GCM with per-message IV |
| **File encryption** | AES-256-GCM with encrypted metadata (filename, size, timestamp) |
| **Key persistence** | localStorage (auto-generated key pair per device) |
| **Key rotation** | Every 24 hours or 1000 messages (whichever comes first) |

### Features

- **1:1 chat** — Operator ↔ end-user device messaging
- **Read receipts** — Message ID arrays for read confirmation
- **Typing indicators** — Real-time typing status
- **Presence** — Online/away/busy status updates
- **File sharing** — Encrypted file transfer up to 50 MB per file
- **Fallback contacts** — If Go server is unavailable, operator + connected agents are returned as contacts

### Message Types

| Type | Direction | Description |
|------|-----------|-------------|
| `key_exchange` | Bidirectional | Public key relay for ECDH |
| `read_receipt` | Bidirectional | Message ID arrays marking read |
| `presence_update` | Bidirectional | Online/away/busy status |
| `file_share` | Bidirectional | Encrypted file metadata relay |
| `chat_message` | Bidirectional | E2E encrypted text message |

---

## 🔓 Unattended Access & Wake-on-LAN

### Unattended Access Management

Administrators can configure unattended access policies per device, controlling who can connect and when.

```bash
# Create an access policy
curl -X POST http://localhost:21114/api/access-policies \
  -H "Authorization: Bearer <jwt>" \
  -H "Content-Type: application/json" \
  -d '{
    "peer_id": "ABC123",
    "password": "secret",
    "enabled": true,
    "allowed_operators": ["operator1", "operator2"],
    "schedule": {
      "days": ["mon", "tue", "wed", "thu", "fri"],
      "start_time": "08:00",
      "end_time": "18:00",
      "timezone": "Europe/Warsaw"
    }
  }'
```

**Features:**
- Set/change device passwords (bcrypt hashed)
- Enable/disable unattended access per device
- Configure access schedules (day/time/timezone)
- Restrict allowed operators
- Web Console UI: modal on device detail page with full schedule editor

### Wake-on-LAN

Power on offline devices remotely via magic packet:

```bash
# Wake device by peer ID (MAC address provided in request body)
curl -X POST http://localhost:21114/api/peers/ABC123/wol \
  -H "Authorization: Bearer <jwt>" \
  -H "Content-Type: application/json" \
  -d '{"mac": "AA:BB:CC:DD:EE:FF"}'
```

The Go server sends a UDP broadcast magic packet (6× `0xFF` + 16× MAC address) to `255.255.255.255:9`. Available from the MGMT desktop client and Web Console (offline devices only).

---

## �🛠️ Technology Stack

### Yomie Go Server

| Component | Technology |
|-----------|-----------|
| **Language** | Go 1.21+ (pure Go, no CGO) |
| **Protocol** | RustDesk rendezvous + relay (protobuf) |
| **Crypto** | NaCl (Curve25519 + secretbox), Ed25519 (`golang.org/x/crypto`) |
| **Database** | SQLite (`modernc.org/sqlite`), PostgreSQL (`pgx/v5` + `pgxpool`) |
| **WebSocket** | `coder/websocket` |
| **Protobuf** | `google.golang.org/protobuf` |
| **TLS** | Standard library `crypto/tls` (min TLS 1.2) |

### Web Console (Node.js)

| Component | Technology |
|-----------|-----------|
| **Runtime** | Node.js 18+ |
| **Framework** | Express.js |
| **Templates** | EJS |
| **Database** | better-sqlite3 |
| **Auth** | bcrypt + TOTP (`otplib`) |
| **Security** | csrf-csrf, Helmet, express-rate-limit |
| **Client API** | Dedicated port with 7-layer security |

### Frontend

| Component | Technology |
|-----------|-----------|
| **UI** | HTML5, CSS3 (glassmorphism, responsive breakpoints), JavaScript ES6+ |
| **Icons** | Material Icons (offline) |
| **Charts** | Live metric bars + history charts |
| **i18n** | JSON-based translations |

### DevOps

| Component | Technology |
|-----------|-----------|
| **Linux services** | systemd (`yomie-server.service` + `yomie-console.service`) |
| **Windows services** | NSSM (`YomieServer` + `YomieConsole`) |
| **Docker** | Docker Compose with local image builds |
| **Installation** | Bash (`yomie.sh`) + PowerShell (`yomie.ps1`) ALL-IN-ONE |
| **CI/CD** | GitHub Actions (multi-platform build) |

---

## 🏗️ Building from Source

### Go Server

```bash
cd yomie-server

# Linux (amd64)
CGO_ENABLED=0 go build -ldflags="-s -w" -o yomie-server .

# Linux (arm64)
CGO_ENABLED=0 GOARCH=arm64 go build -ldflags="-s -w" -o yomie-server-arm64 .

# Windows
CGO_ENABLED=0 GOOS=windows GOARCH=amd64 go build -ldflags="-s -w" -o yomie-server.exe .
```

### Migration Tool

```bash
cd yomie-server/tools/migrate
go build -o migrate .
```

### Node.js Console

```bash
cd web-nodejs
npm install
npm start
```

---

## 🌍 Internationalization (i18n)

Yomie Console supports 25+ languages through JSON-based translations with auto-discovery.

**Included languages**: 🇬🇧 English, 🇵🇱 Polish, 🇩🇪 German, 🇫🇷 French, 🇪🇸 Spanish, 🇮🇹 Italian, 🇵🇹 Portuguese, 🇳🇱 Dutch, 🇨🇳 Chinese (Simplified), 🇯🇵 Japanese, 🇰🇷 Korean, 🇸🇦 Arabic, 🇮🇱 Hebrew, 🇺🇦 Ukrainian, 🇹🇷 Turkish, 🇮🇳 Hindi, 🇸🇪 Swedish, 🇳🇴 Norwegian, 🇩🇰 Danish, 🇫🇮 Finnish, 🇨🇿 Czech, 🇭🇺 Hungarian, 🇷🇴 Romanian, 🇹🇭 Thai, 🇻🇳 Vietnamese, 🇮🇩 Indonesian

Languages are auto-discovered from the `lang/` directory — no code changes needed.

**Adding a new language**:
1. Copy `web-nodejs/lang/en.json` → `web-nodejs/lang/{code}.json`
2. Translate all string values
3. Update the `_meta` section with language info
4. Run `npm run i18n:check --fix` to validate 100% key coverage

See [Contributing Translations](docs/development/CONTRIBUTING_TRANSLATIONS.md) for details.

---

## 🤖 CDAP — Connected Device Automation Protocol

CDAP enables managing IoT, ICS, and custom devices alongside standard RustDesk desktops.

### Architecture

```
CDAP Device (Agent/Bridge)  ──WebSocket──►  Go Server (:21122/cdap)
                                              │
                                     REST API + WS proxy
                                              │
                                      Node.js Console ──► Browser
```

### Features

- **Widget rendering** — 8 widget types (gauge, toggle, button, LED, text, slider, select, chart)
- **Real-time telemetry** — Heartbeat with CPU/memory/disk metrics, bulk state updates
- **Remote commands** — Send commands to devices with confirmation and cooldown
- **Terminal** — Browser-based terminal sessions to CDAP agents
- **File browser** — Browse and transfer files from CDAP agents
- **Clipboard sync** — Bidirectional clipboard between browser and device
- **Audio streaming** — Two-way audio via WebSocket (PCM + Opus)
- **Multi-monitor** — Remote display selection and switching

### SDKs & Bridges

| SDK | Language | Status |
|-----|----------|--------|
| [Python SDK](sdks/python/) | Python 3.9+ | ✅ Stable |
| [Node.js SDK](sdks/nodejs/) | Node.js 18+ | ✅ Stable |

| Bridge | Protocol | Status |
|--------|----------|--------|
| [Modbus](bridges/modbus/) | Modbus TCP/RTU | ✅ Reference |
| [SNMP](bridges/snmp/) | SNMP v2c/v3 | ✅ Reference |
| [REST/Webhook](bridges/rest-webhook/) | HTTP polling + webhooks | ✅ Reference |

See [CDAP Documentation](docs/cdap/OVERVIEW.md) and [SDK Documentation](docs/sdk/OVERVIEW.md) for details.

---

## 🖥️ Desktop Clients

### Yomie MGMT Client (Operator/Admin)

A Tauri v2 + SolidJS desktop application for operators and administrators.

- Operator login with TOTP 2FA and JWT token management
- Device list with live status, search, filter, group chips, and CPU/RAM metrics
- One-click remote connect to any device (H.264/VP9 video pipeline)
- Full remote desktop with multi-monitor selection, clipboard sync, file transfer, session recording
- Help request management inbox with accept & auto-connect
- Session history dashboard with audit trail
- Server management panel (admin) — clients, operators, audit, API keys, config
- Notification center with real-time push (30s polling)
- Unattended access management — set passwords, schedules, operator restrictions
- Wake-on-LAN for offline devices
- Device actions — restart, shutdown, lock screen, log off, send message
- File transfer panel with local file browsing
- Input injection via `enigo` crate (keyboard, mouse, text)
- Single-instance enforcement (Windows mutex)
- Full i18n (English + Polish, ~60 keys)

**Build**: `cd yomie-mgmt && pnpm install && pnpm tauri build`

### Yomie Agent Client (Endpoint Device)

A lightweight Tauri v2 agent installed on end-user devices.

- 5-step setup wizard with sequential server validation (availability → protocol → registration → certificate → complete)
- Device registration with machine UID-based device ID (`BD-{hash}`) and OS keyring token storage
- System info collection (hostname, OS, CPU, RAM, disk, username)
- Help request submission (4-state flow: idle → composing → sending → sent)
- Operator chat with E2E encryption
- Settings panel (connection, privacy, general, about)
- Tray icon with autostart, single-instance enforcement
- Minimal UI (480×520 single window)
- Full i18n (English + Polish, ~120 keys)

**Build**: `cd yomie-agent-client && pnpm install && pnpm tauri build`

### Native CDAP Agent (Go)

A headless Go binary for servers and IoT devices.

- 9 system widgets (CPU, memory, disk, hostname, uptime)
- Terminal, file browser, clipboard, screenshot capabilities
- Systemd / NSSM service installers

**Build**: `cd yomie-agent && go build -o yomie-agent .`

---

## 🌐 Web Remote Desktop

Yomie includes a browser-based remote desktop client accessible from the Web Console.

### Features

| Feature | HTTPS | HTTP |
|---------|:-----:|:----:|
| **Video codec** | WebCodecs (H.264, VP9, AV1, VP8) | JMuxer (H.264 only) |
| **Target FPS** | 60 fps | 30 fps |
| **Image quality** | Best (configurable) | Best (configurable) |
| **Session recording** | ✅ WebM VP9+Opus at 15fps | ✅ |
| **Monitor switching** | ✅ Display list with resolutions | ✅ |
| **Quality presets** | Speed / Balanced / Quality / Best | Same |
| **Keyboard/mouse** | ✅ Full input (modifiers, F-keys, wheel) | ✅ |
| **Clipboard sync** | ✅ Bidirectional via navigator.clipboard | ❌ (HTTPS required) |
| **Special keys** | Ctrl+Alt+Del, Win, PrintScreen, Alt+Tab | Same |
| **Fullscreen** | ✅ F11 / button toggle | ✅ |
| **Scale modes** | Fit / Fill / Original (1:1) | Same |

### Session Recording

```javascript
// Start recording during a remote session
client.startRecording();  // Canvas capture stream + audio at 15fps, WebM VP9+Opus

// Stop and auto-download
client.stopRecording();   // Produces .webm file download
```

### Quality Presets

| Preset | Image Quality | FPS | Use Case |
|--------|:------------:|:---:|----------|
| **Speed** | Low | 30 | Slow connections, text work |
| **Balanced** | Balanced | 30 | General use |
| **Quality** | Quality | 60 | Design work, presentations |
| **Best** | Best | 60 | LAN, maximum fidelity |

---

## 📂 Project Structure

```
Rustdesk-FreeConsole/
├── yomie-server/           # Go server (~20K LOC)
│   ├── main.go                  # Entry point, flags, boot sequence
│   ├── signal/                  # Signal server (UDP/TCP/WS)
│   ├── relay/                   # Relay server (TCP/WS)
│   ├── api/                     # HTTP REST API + auth handlers
│   ├── crypto/                  # Ed25519, NaCl, Curve25519, address codec
│   ├── db/                      # Database interface + SQLite + PostgreSQL
│   ├── config/                  # Configuration, constants, TLS, DualModeListener
│   ├── codec/                   # Wire protocol framing (TCP + WS)
│   ├── peer/                    # Concurrent in-memory peer map (4-tier status)
│   ├── security/                # IP/ID/CIDR blocklist
│   ├── auth/                    # JWT, PBKDF2, RBAC roles, TOTP 2FA
│   ├── ratelimit/               # IP limiter, bandwidth limiter, conn limiter
│   ├── metrics/                 # Prometheus text exposition
│   ├── audit/                   # Ring-buffer audit log
│   ├── events/                  # Pub/sub event bus (11 event types)
│   ├── logging/                 # Text/JSON structured logging
│   ├── admin/                   # TCP management console
│   ├── reload/                  # Hot-reload (SIGHUP / manual)
│   ├── proto/                   # Generated protobuf (rendezvous + message)
│   └── tools/migrate/           # Migration tool (5 modes)
├── web-nodejs/                  # Node.js web console
│   ├── server.js                # Express app (dual port: 5000 + 21121)
│   ├── routes/                  # API routes (panel + Client API)
│   ├── middleware/              # Security, CSRF, WAN, i18n, rate limiting
│   ├── services/                # Auth, DB, WebSocket relay
│   ├── views/                   # EJS templates
│   ├── public/                  # Static assets (CSS, JS)
│   └── lang/                    # i18n translations (EN, PL)
├── yomie-mgmt/             # MGMT Desktop Client (Tauri v2 + SolidJS)
├── yomie-agent-client/     # Agent Client (Tauri v2 + SolidJS)
├── yomie-agent/            # Native CDAP Agent (Go)
├── sdks/                        # CDAP Bridge SDKs (Python + Node.js)
├── bridges/                     # Reference CDAP Bridges (Modbus, SNMP, REST)
├── yomie.sh                # Linux ALL-IN-ONE installer (v2.4.0)
├── yomie.ps1               # Windows ALL-IN-ONE installer (v2.4.0)
├── yomie-docker.sh         # Docker installer (v2.4.0)
├── docker-compose.yml           # Docker orchestration
├── docs/                        # Documentation (architecture, CDAP, SDK, security)
├── dev_modules/                 # Development & testing utilities
└── archive/                     # Archived: Rust binaries, Flask console
```

---

## 🤝 Contributing

Contributions are welcome! See [CONTRIBUTING.md](docs/development/CONTRIBUTING.md) for guidelines.

### Reporting Issues

1. Run diagnostics: `sudo ./yomie.sh` → option 8
2. Collect logs: `journalctl -u yomie-server -n 100`
3. Open a [GitHub Issue](https://github.com/shamstabraiz/Rustdesk-FreeConsole/issues) with system info, logs, and reproduction steps

### Pull Requests

1. Fork → branch (`feature/your-feature`) → commit → push → PR
2. Follow existing code style and conventions
3. Update documentation and i18n keys for new features
4. Test with real RustDesk clients on both Linux and Windows

---

## 📄 License

This project is licensed under the **Apache License 2.0** — see [LICENSE](LICENSE).

### Clean-Room Implementation

Yomie Server (`yomie-server/`) is a **clean-room implementation** of the RustDesk rendezvous and relay protocol. Like how any HTTP server implements the HTTP protocol without being "derived from" Apache or Nginx, Yomie implements published protocol specifications for compatibility with RustDesk clients — but contains **no RustDesk source code**.

- **Go imports**: No `github.com/rustdesk/*` dependencies
- **Code review**: No RustDesk copyright headers or attribution
- **Protocol**: Uses independently authored `.proto` specifications with Apache 2.0 headers

Yomie Console (`web-nodejs/`) is an entirely original Node.js/Express application.

### Archive Directory

The `archive/` directory (excluded from distribution via `.gitignore`) contains deprecated code from earlier development phases, including legacy Rust-based server code licensed under AGPL-3.0. These files are **not covered by the Apache 2.0 license** and are never included in releases, packages, or Docker images.

### Trademark Notice

"RustDesk" is a trademark of the RustDesk Team. Yomie is an independent project that implements the RustDesk protocol for client compatibility. Use of the name "RustDesk" in this project is purely descriptive (indicating protocol compatibility) and does not imply affiliation with or endorsement by the RustDesk Team.

### Commercial License

Commercial licensing is available for organizations requiring extended support, white-label / OEM redistribution rights, or custom integrations — contact shamstabraiz for details.

---

## 🙏 Credits

- **[RustDesk](https://github.com/rustdesk/rustdesk)** — The open-source remote desktop solution
- **[RustDesk Server](https://github.com/rustdesk/rustdesk-server)** — Original HBBS/HBBR reference implementation
- **[pgx](https://github.com/jackc/pgx)** — PostgreSQL driver for Go
- **[coder/websocket](https://github.com/coder/websocket)** — WebSocket library for Go
- **[modernc.org/sqlite](https://modernc.org/sqlite)** — Pure Go SQLite driver
- **[Material Icons](https://fonts.google.com/icons)** — Google Material Design icons

---

## 📞 Support

- **Documentation**: [docs/](docs/)
- **Issues**: [GitHub Issues](https://github.com/shamstabraiz/Rustdesk-FreeConsole/issues)
- **Discussions**: [GitHub Discussions](https://github.com/shamstabraiz/Rustdesk-FreeConsole/discussions)

---

<div align="center">

**Made with ❤️ by shamstabraiz & the community**

If you find this project useful, please consider giving it a ⭐ on GitHub!

[⬆ Back to Top](#-yomie--rustdesk-compatible-server--web-console)

</div>

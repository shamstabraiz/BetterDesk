# Yomie — Encryption Specification

> **Version:** 1.0
> **Last Updated:** 2026-04-01

## Overview

Yomie uses layered encryption to protect data in transit and at rest across all communication channels.

## Algorithms

| Purpose | Algorithm | Key Size | Notes |
|---------|-----------|----------|-------|
| Server identity | Ed25519 | 256-bit | Sign peer registrations, verify server identity |
| Key exchange (P2P) | X25519 (Curve25519 DH) | 256-bit | Derive shared secret between peers |
| Symmetric encryption (P2P) | XSalsa20-Poly1305 (NaCl box) | 256-bit key, 192-bit nonce | Authenticated encryption for peer-to-peer |
| TLS transport | TLS 1.2+ (auto-negotiated) | Depends on cipher suite | Signal, relay, API server connections |
| Password hashing (admin) | bcrypt | Cost factor 12 | Web console admin accounts |
| Password hashing (org users) | PBKDF2-SHA256 | 100K iterations, 32-byte salt | Organization user accounts |
| JWT signing | HMAC-SHA256 | 256-bit | API authentication tokens |
| TOTP 2FA | HMAC-SHA1 (RFC 6238) | 160-bit shared secret | 6-digit codes, 30-second step |
| Session secret | Random bytes | 256-bit | Express session signing |
| API key | Cryptographically random hex | 256-bit (32 bytes) | Server ↔ console authentication |
| CSRF token | Double-submit cookie | 256-bit | Cross-site request forgery prevention |

## Connection Encryption Matrix

| Connection | Transport | Application Layer | Forward Secrecy |
|------------|-----------|-------------------|-----------------|
| Client ↔ Signal Server (TCP) | Optional TLS (`--tls-signal`) | NaCl secure channel (key exchange) | Yes (ephemeral DH) |
| Client ↔ Signal Server (UDP) | None (UDP) | Protobuf (unencrypted metadata) | No |
| Client ↔ Relay Server (TCP) | Optional TLS (`--tls-relay`) | Transparent relay (E2E between peers) | Via P2P layer |
| Client ↔ Client (P2P) | Direct TCP | NaCl box (X25519 + XSalsa20-Poly1305) | Yes (per-session DH) |
| Browser ↔ Web Console | HTTPS (recommended) | Session cookie + CSRF token | Via TLS |
| Console ↔ Go Server API | HTTP localhost | API key header | No (planned: mTLS) |
| CDAP Agent ↔ Gateway | WebSocket (WSS recommended) | API key authentication | Via TLS |

## Key Management

### Ed25519 Server Keys
- Generated on first server startup
- Stored in `id_ed25519` / `id_ed25519.pub` files
- File permissions: `0600` (owner read/write only)
- No automatic rotation (server identity key)
- Clients verify server public key against stored value

### API Keys
- Auto-generated on first run (32 bytes, hex-encoded)
- Stored in `.api_key` file + synced to `server_config` database table
- Transmitted via `X-API-Key` HTTP header (never in URL)
- Manual rotation via admin panel or file replacement

### Session Secrets
- Random 64-byte hex string generated on first console startup
- Stored in `.env` file (`SESSION_SECRET` variable)
- Used to sign Express session cookies

### TOTP Secrets
- Per-user, generated during 2FA enrollment
- Stored encrypted in database (`totp_secret` column)
- Recovery codes: 10 single-use codes generated at enrollment

## TLS Configuration

### Server-side (Go)
- Dual-mode listeners: auto-detect plain TCP vs TLS on same port (first-byte `0x16` detection)
- Minimum TLS version: 1.2
- Flags: `--tls-cert`, `--tls-key`, `--tls-signal`, `--tls-relay`, `--tls-api`
- Self-signed certificates supported (client must trust CA)

### Client-side (Tauri/Rust)
- Trusts system certificate store by default
- Certificate pinning planned for Phase 8
- WebSocket connections use `wss://` when server HTTPS detected

## Data at Rest

| Data | Encryption | Location |
|------|-----------|----------|
| Database (SQLite) | None (filesystem permissions) | `db_v2.sqlite3` |
| Database (PostgreSQL) | TLS in transit, at-rest depends on PG config | PostgreSQL server |
| Ed25519 private key | None (file permissions `0600`) | `id_ed25519` |
| Admin password hash | bcrypt (irreversible) | `auth.db` |
| TOTP secrets | Stored as-is in DB | `auth.db` / `users` table |
| Session data | Server-side storage (not in cookie) | Memory / session store |

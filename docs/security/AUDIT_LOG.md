# Yomie — Security Audit Log

> Chronological record of security audits, findings, and resolutions.

---

## Audit #1 — Web Console Security Review (2026-02-17)

**Scope:** Node.js web console (`web-nodejs/`)
**Conducted by:** Internal review (GitHub Copilot assisted)

### Findings

| ID | Severity | Finding | Resolution | Status |
|----|----------|---------|------------|--------|
| A1-C1 | Critical | No CSRF protection on state-changing routes | Added double-submit cookie pattern with `csrf-csrf` | ✅ Fixed |
| A1-C2 | Critical | Session fixation — session not regenerated after login | Added `req.session.regenerate()` after successful authentication | ✅ Fixed |
| A1-C3 | Critical | Timing attack on login — early return for non-existent users | Pre-computed dummy bcrypt hash compared for all attempts | ✅ Fixed |
| A1-H1 | High | WebSocket connections not authenticated | Added session cookie verification for WS upgrade | ✅ Fixed |
| A1-H2 | High | RustDesk Client API exposed on panel port | Moved to dedicated WAN-facing port 21121 with 7-layer security | ✅ Fixed |
| A1-H3 | High | No 2FA support | Added TOTP 2FA with `otplib` | ✅ Fixed |
| A1-M1 | Medium | Trust proxy not configurable | Added `TRUST_PROXY` env var | ✅ Fixed |
| A1-M2 | Medium | Session cookie missing Secure flag on HTTPS | Auto-set Secure flag when HTTPS detected | ✅ Fixed |

## Audit #2 — Go Server Security Review (2026-02-28)

**Scope:** Go server (`yomie-server/`)

### Findings

| ID | Severity | Finding | Resolution | Status |
|----|----------|---------|------------|--------|
| A2-H1 | High | No validation on `new_id` in change-id API | Added `peerIDRegexp` validation | ✅ Fixed |
| A2-H3 | High | No rate limiting on 2FA endpoint | Added `loginLimiter.Allow(clientIP)` | ✅ Fixed |
| A2-H4 | High | Partial 2FA token has no short TTL | Added `GenerateWithTTL()` with 5-min expiry | ✅ Fixed |
| A2-M1 | Medium | SQL LIKE wildcard injection in ListPeersByTag | Added `ESCAPE '\'` clause, escape `%` and `_` | ✅ Fixed |
| A2-M4 | Medium | No rate limiting on TCP signal connections | Added `limiter.Allow(host)` in `serveTCP()` | ✅ Fixed |
| A2-M6 | Medium | No validation on config key names | Added `configKeyRegexp` (1-64 chars, alnum/dots/hyphens) | ✅ Fixed |

## Audit #3 — Node.js Console Security Review (2026-02-28)

**Scope:** RustDesk Client API on Node.js (`web-nodejs/`)

### Findings

| ID | Severity | Finding | Resolution | Status |
|----|----------|---------|------------|--------|
| A3-H1 | High | Rate limiter uses X-Forwarded-For without validation | Switched to `req.ip` (respects trust proxy setting) | ✅ Fixed |
| A3-H2 | High | Device verification token replay possible | Added nonce + timestamp validation | ✅ Fixed |
| A3-M4 | Medium | Missing input length validation on sysinfo fields | Added 255-char limit on hostname, platform, version | ✅ Fixed |

## Audit #4 — Installer Scripts Security Review (2026-03-15)

**Scope:** `yomie.sh`, `yomie.ps1`

### Findings

| ID | Severity | Finding | Resolution | Status |
|----|----------|---------|------------|--------|
| A4-H1 | High | SQL injection in password reset via shell interpolation | Replaced with env-var passing to Python/Node | ✅ Fixed |
| A4-M1 | Medium | Plaintext admin credentials persisted by default | Made opt-in via `STORE_ADMIN_CREDENTIALS=true` | ✅ Fixed |
| A4-M2 | Medium | `npm audit` showed tar vulnerability | Added override in package.json | ✅ Fixed |

---

*New audits should be appended below with incrementing audit numbers.*

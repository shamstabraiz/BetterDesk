# Login and API Security Audit

Date: 2026-04-26
Scope: Yomie login flows, session handling, token handling, and HTTP/WebSocket API authorization in the Node.js console and Go server
Method: source-level audit of the current main branch, focused on authentication, authorization, rate limiting, token lifecycle, and browser/API trust boundaries
Status: Patches 1–4 applied on 2026-04-26 (low/zero compatibility risk). Patches 5–6 are deferred and tracked separately because they require a phased rollout to avoid disrupting existing installations.

### Remediation Status (2026-04-26)

| # | Finding | Severity | Status | Notes |
|---|---|---|---|---|
| 1 | Brute-force lockout bypass (missing `await`) | High | **Fixed** | `web-nodejs/routes/rustdesk-api.routes.js` — added `await` before `authService.checkBruteForce`. No schema, role or token impact. |
| 2 | TOTP completion without session regeneration | Medium | **Fixed** | `web-nodejs/routes/auth.routes.js` — wrapped post-TOTP session writes in `req.session.regenerate(...)`, mirroring the standard login flow. Cookie name unchanged; existing sessions continue to work. |
| 3 | Audit / WS-events endpoints lacked explicit RBAC | Medium | **Fixed** | `yomie-server/api/server.go` — wrapped `GET /api/audit/events` and `GET /api/ws/events` in `requirePermission(auth.PermAuditView, ...)`. All built-in roles that previously consumed these endpoints (super_admin, admin, server_admin, global_admin, operator, viewer) already grant `audit.view`; only the `pro` role loses access — see CHANGELOG. |
| 4 | Raw internal `err.Error()` strings leaked from Go auth handlers | Medium | **Fixed** | `yomie-server/api/auth_handlers.go` — nine 500-paths now return `{"error":"internal error"}` while logging full detail server-side via `log.Printf`. Status codes preserved; non-500 paths unchanged. |
| 5 | Plaintext access tokens in `access_tokens` table | Medium | **Deferred** | Requires three-phase rollout to avoid forcing every active RustDesk client to re-authenticate. Tracked in section *Deferred Patches* below. |
| 6 | CSP still uses `'unsafe-inline'` (script attrs) and `'unsafe-eval'` (remote viewer) | Low | **Deferred** | Removing these flags requires refactoring inline EJS event handlers and replacing `eval`-based protobuf.js code generation. Tracked in section *Deferred Patches* below. |

## Executive Summary

Yomie has a solid security baseline for a multi-component remote management platform. The project already includes session-based authentication for the web panel, TOTP 2FA, CSRF protection, rate limiting, RBAC, API keys, JWTs, and a reasonable amount of regression coverage.

The main weakness is not the complete absence of protections, but inconsistency between several authentication paths:

- the web panel login flow
- the RustDesk-compatible API exposed by the Node.js console
- the Go server admin/API layer

As of this audit, the strongest path is the standard web-panel login flow. The highest-risk issues are concentrated around the RustDesk-facing Node.js API and permission enforcement inconsistencies in the Go API.

Overall assessment: Medium-High maturity with several actionable findings that should be addressed before treating the login and API surface as fully hardened.

## Scope Reviewed

The audit focused on the following areas:

- Node.js web authentication routes in [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js)
- Node.js RustDesk-compatible API in [web-nodejs/routes/rustdesk-api.routes.js](../../web-nodejs/routes/rustdesk-api.routes.js)
- Node.js auth service and token handling in [web-nodejs/services/authService.js](../../web-nodejs/services/authService.js)
- Node.js database/token persistence in [web-nodejs/services/database.js](../../web-nodejs/services/database.js) and [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js)
- Node.js security middleware in [web-nodejs/middleware/auth.js](../../web-nodejs/middleware/auth.js), [web-nodejs/middleware/csrf.js](../../web-nodejs/middleware/csrf.js), [web-nodejs/middleware/security.js](../../web-nodejs/middleware/security.js), and [web-nodejs/middleware/rateLimiter.js](../../web-nodejs/middleware/rateLimiter.js)
- Go server auth and routing in [yomie-server/api/auth_handlers.go](../../yomie-server/api/auth_handlers.go), [yomie-server/api/server.go](../../yomie-server/api/server.go), [yomie-server/api/client_api_handlers.go](../../yomie-server/api/client_api_handlers.go), [yomie-server/api/token_handlers.go](../../yomie-server/api/token_handlers.go)
- Go auth primitives in [yomie-server/auth/jwt.go](../../yomie-server/auth/jwt.go), [yomie-server/auth/password.go](../../yomie-server/auth/password.go), and [yomie-server/auth/totp.go](../../yomie-server/auth/totp.go)
- Related tests in [web-nodejs/tests/auth.routes.test.js](../../web-nodejs/tests/auth.routes.test.js), [web-nodejs/tests/middleware.auth.test.js](../../web-nodejs/tests/middleware.auth.test.js), [web-nodejs/tests/security.middleware.test.js](../../web-nodejs/tests/security.middleware.test.js), [yomie-server/auth/password_test.go](../../yomie-server/auth/password_test.go), [yomie-server/auth/jwt_test.go](../../yomie-server/auth/jwt_test.go), and [yomie-server/auth/totp_test.go](../../yomie-server/auth/totp_test.go)

## Security Strengths

### 1. Good baseline web-panel session security

The standard web login flow regenerates the session after successful authentication in [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L106). This is the correct defense against session fixation in the primary panel flow.

### 2. CSRF protection is implemented correctly for browser traffic

The panel uses the double-submit cookie pattern via csrf-csrf in [web-nodejs/middleware/csrf.js](../../web-nodejs/middleware/csrf.js#L24), which is materially stronger than ad hoc form-token logic.

### 3. User enumeration resistance exists in Node auth

The Node auth service uses a precomputed dummy bcrypt hash in [web-nodejs/services/authService.js](../../web-nodejs/services/authService.js#L18), so username misses still pay a realistic password-check cost.

### 4. Input length limiting exists on the web login edge

The panel rejects oversized username/password inputs in [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L61), reducing the impact of cost-amplification attacks against bcrypt.

### 5. Go API rate limiting exists for login and 2FA

The Go login and login/2fa handlers both enforce per-IP rate limiting in [yomie-server/api/auth_handlers.go](../../yomie-server/api/auth_handlers.go#L177) and [yomie-server/api/auth_handlers.go](../../yomie-server/api/auth_handlers.go#L250).

### 6. Go partial 2FA tokens are short-lived

The Go API correctly issues a 5-minute partial token for the 2FA step in [yomie-server/api/auth_handlers.go](../../yomie-server/api/auth_handlers.go#L213). That is a strong default compared with long-lived intermediate auth tokens.

### 7. API key transport is tighter than the historical baseline

The Go API explicitly accepts API keys only from the X-API-Key header, not from query parameters, in [yomie-server/api/auth_handlers.go](../../yomie-server/api/auth_handlers.go#L801). This reduces leakage into logs, caches, and reverse proxies.

### 8. Go client-facing heartbeat/sysinfo endpoints are rate-limited

The server defines and uses a dedicated limiter for heartbeat/sysinfo behavior in [yomie-server/api/server.go](../../yomie-server/api/server.go#L76), [yomie-server/api/client_api_handlers.go](../../yomie-server/api/client_api_handlers.go#L563), [yomie-server/api/client_api_handlers.go](../../yomie-server/api/client_api_handlers.go#L631), and [yomie-server/api/client_api_handlers.go](../../yomie-server/api/client_api_handlers.go#L708).

## Findings Summary

| Severity | Area | Status | Summary |
|---|---|---|---|
| High | Node RustDesk API | **Fixed (2026-04-26)** | Brute-force protection is bypassed because an async lockout check is called without await |
| Medium | Node web panel | **Fixed (2026-04-26)** | TOTP completion path sets an authenticated session without regenerating it |
| Medium | Go API RBAC | **Fixed (2026-04-26)** | Audit events and event-stream endpoints are not wrapped in explicit audit.view permission checks |
| Medium | Go API error handling | **Fixed (2026-04-26)** | Several auth/admin handlers return raw internal err.Error strings to clients |
| Medium | Node token storage | **Deferred** | RustDesk client access tokens are stored in plaintext in the auth database |
| Low | Node CSP hardening | **Deferred** | The console still uses scoped compatibility exceptions such as unsafe-inline for script attributes and unsafe-eval for the remote viewer |

## Detailed Findings

### Finding 1: Brute-force lockout is bypassed in the Node RustDesk API

Severity: High

Evidence:

- [web-nodejs/routes/rustdesk-api.routes.js](../../web-nodejs/routes/rustdesk-api.routes.js#L828)
- [web-nodejs/services/authService.js](../../web-nodejs/services/authService.js#L705)

Description:

The RustDesk-compatible login route in the Node.js console calls authService.checkBruteForce without await:

- it stores the Promise in bruteCheck
- it immediately reads bruteCheck.blocked

Because checkBruteForce is asynchronous, the intended account/IP lockout result is not actually awaited. This means the route can continue into authentication even when the lockout logic should have blocked the request.

Impact:

- account lockouts and IP-based throttling can be silently bypassed on the RustDesk-facing Node login path
- the project effectively has weaker protection on the WAN/API login path than on the standard browser panel path
- the inconsistency makes security assumptions about shared authService protections unreliable

Recommended remediation:

- change the route to await authService.checkBruteForce(username, ip)
- add a regression test covering blocked account and blocked IP behavior specifically for the RustDesk API login route

### Finding 2: Web-panel 2FA completion does not regenerate the session

Severity: Medium

Evidence:

- session regeneration during password-only login: [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L106)
- TOTP completion flow: [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L268) and [web-nodejs/routes/auth.routes.js](../../web-nodejs/routes/auth.routes.js#L272)

Description:

The standard web login route regenerates the session correctly after successful username/password authentication. However, when TOTP is required, the final verification path only clears the pending fields and writes the authenticated user into the current session object.

Impact:

- session fixation protection is complete only for non-2FA logins
- accounts with 2FA enabled can still inherit an existing session identifier through the second phase of the login flow
- this weakens one of the most important protections for privileged accounts

Recommended remediation:

- regenerate the session again after successful TOTP verification before setting req.session.userId and req.session.user
- add a regression test that verifies the session ID changes across the TOTP-completion step

### Finding 3: Go audit/event endpoints are not protected by the dedicated audit permission

Severity: Medium

Evidence:

- audit permission exists: [yomie-server/auth/permissions.go](../../yomie-server/auth/permissions.go#L36)
- audit route registration: [yomie-server/api/server.go](../../yomie-server/api/server.go#L201)
- websocket events route registration: [yomie-server/api/server.go](../../yomie-server/api/server.go#L204)
- audit handler: [yomie-server/api/server.go](../../yomie-server/api/server.go#L1190)
- event types broadcast: [yomie-server/events/bus.go](../../yomie-server/events/bus.go)

Description:

The Go server defines a dedicated audit.view permission, but the main audit-events endpoint and the event-stream websocket are not wrapped in requirePermission(auth.PermAuditView, ...). They are only protected by the generic auth middleware.

Impact:

- authenticated users may gain visibility into operational or security telemetry that should be reserved for audit-capable roles
- this undermines the intended RBAC model and makes audit visibility harder to reason about

Recommended remediation:

- wrap GET /api/audit/events with requirePermission(auth.PermAuditView, ...)
- decide whether GET /api/ws/events should require audit.view, a more specific event-stream permission, or a filtered per-role policy
- add Go route-level tests for forbidden access by lower-privilege roles

### Finding 4: Go auth/admin handlers leak internal error strings

Severity: Medium

Evidence:

- [yomie-server/api/auth_handlers.go](../../yomie-server/api/auth_handlers.go#L336)
- [yomie-server/api/auth_handlers.go](../../yomie-server/api/auth_handlers.go#L498)
- [yomie-server/api/auth_handlers.go](../../yomie-server/api/auth_handlers.go#L743)

Description:

Several handlers return err.Error() directly in 500 responses. This is an information disclosure pattern: internal storage or validation failures become externally visible to any already-authenticated caller that can reach these routes.

Impact:

- database-layer details, uniqueness violations, and internal runtime errors can leak to clients
- post-auth recon becomes easier for attackers operating with stolen credentials or a leaked API key

Recommended remediation:

- replace raw err.Error responses with generic messages such as internal error or operation failed
- log the real error server-side with enough context for operators

### Finding 5: Node RustDesk access tokens are stored in plaintext

Severity: Medium

Evidence:

- schema declaration: [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L253) and [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L255)
- token creation and lookup: [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L1147) and [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L1154)

Description:

RustDesk client access tokens are generated with good entropy, but they are stored directly in the database rather than as a one-way hash. A read-only database compromise is therefore enough to replay any live token until expiry or revocation.

Impact:

- database disclosure becomes session disclosure
- the blast radius of backups, snapshots, or accidental DB exposure is larger than necessary

Recommended remediation:

- store only a SHA-256 or HMAC-based token hash
- compare hashes on lookup
- preserve the plaintext token only in memory at issuance time

### Finding 6: CSP still includes compatibility exceptions

Severity: Low

Evidence:

- remote viewer unsafe-eval exception: [web-nodejs/middleware/security.js](../../web-nodejs/middleware/security.js#L27)
- inline script attribute exception: [web-nodejs/middleware/security.js](../../web-nodejs/middleware/security.js#L37)

Description:

The console CSP is better than a default Express app, but it still carries compatibility exceptions for specific UI paths. These are understandable tradeoffs, not proof of active XSS, but they reduce defense-in-depth.

Impact:

- any future XSS bug in affected pages will be easier to weaponize
- the remote-viewer page in particular has a narrower CSP margin than the rest of the panel

Recommended remediation:

- continue removing inline handler usage from templates and client rendering
- replace the protobuf/runtime dependency path that currently requires unsafe-eval in the remote viewer

## Architectural Observation

The project currently maintains multiple authentication models in parallel:

- browser session auth for the web console
- DB-backed opaque access tokens for the Node RustDesk API
- JWT and scoped API-key auth in the Go server

This is not inherently wrong, but it materially increases the risk of drift. The highest-value theme from this audit is consistency: the biggest problems are where one path is more secure than another, even though both are supposed to represent the same product trust boundary.

## Test Coverage Assessment

Positive signals:

- there are dedicated tests for panel auth and middleware in [web-nodejs/tests/auth.routes.test.js](../../web-nodejs/tests/auth.routes.test.js), [web-nodejs/tests/middleware.auth.test.js](../../web-nodejs/tests/middleware.auth.test.js), and [web-nodejs/tests/security.middleware.test.js](../../web-nodejs/tests/security.middleware.test.js)
- there are focused auth primitive tests in [yomie-server/auth/password_test.go](../../yomie-server/auth/password_test.go), [yomie-server/auth/jwt_test.go](../../yomie-server/auth/jwt_test.go), and [yomie-server/auth/totp_test.go](../../yomie-server/auth/totp_test.go)

Coverage gaps identified by this audit:

- no regression test appears to cover the RustDesk API brute-force lockout path in Node
- no regression test appears to verify session regeneration across the TOTP completion flow in Node
- no Go route-level authorization test appears to enforce audit.view on audit/events endpoints because the wrapper is currently absent
- no test currently enforces hashed-at-rest behavior for Node RustDesk access tokens because the tokens are still stored in plaintext

## Recommended Remediation Order

### Priority 1

- fix the missing await in the Node RustDesk API brute-force check
- regenerate the session after successful TOTP verification in the web-panel flow
- protect Go audit/events endpoints with explicit RBAC

### Priority 2

- remove raw err.Error leakage from Go auth/admin handlers
- hash Node RustDesk access tokens at rest

### Priority 3

- continue tightening CSP exceptions in the console and remote viewer
- consider longer-term auth-model unification across Node panel, Node RustDesk API, and Go API

## Conclusion

The Yomie login and API surfaces are clearly more mature than an average custom remote-management stack. The project already applies many of the right primitives and patterns. The most important work now is to eliminate inconsistent security behavior between different access paths.

If the Priority 1 items are fixed, the overall risk posture of the login/API layer will improve substantially without any architectural rewrite.

## Deferred Patches

The following two findings are intentionally **not** fixed in this batch because a naive patch would break existing Yomie installations. They are documented here so they can be implemented later as standalone, well-tested rollouts.

### Patch 5 — Hash RustDesk access tokens at rest (Medium)

Why it is deferred: every active RustDesk client today holds a token whose plaintext value is stored in [web-nodejs/services/dbAdapter.js](../../web-nodejs/services/dbAdapter.js#L1140-L1160). Replacing the column with a hash in a single commit invalidates every existing client session.

Required rollout:

1. **Phase 1 (additive, backward compatible):** Add a `token_hash TEXT` column to `access_tokens`. On token creation, write both `token` (plaintext, legacy) and `token_hash`. On lookup, try `token_hash` first, fall back to `token`.
2. **Phase 2 (drain):** Wait one token TTL (default 30 days) so all live tokens have been re-issued under the dual-write scheme. Add log warning whenever the legacy plaintext lookup branch is hit.
3. **Phase 3 (enforce):** Drop the plaintext `token` column, remove the fallback branch, and enforce `token_hash NOT NULL`.

Until Phase 3 is complete, restrict filesystem permissions on the auth database file as a compensating control.

### Patch 6 — Remove `'unsafe-inline'` script attrs and `'unsafe-eval'` from CSP (Low)

Why it is deferred: removing these flags is not a configuration change \u2014 it is a refactor.

Required work:

1. Audit every EJS template under `web-nodejs/views/` for inline `onclick=`, `onsubmit=`, `onload=` and similar handlers; move them to delegated event listeners in JS modules.
2. Replace the `protobuf.js` runtime path used by the remote viewer (which currently requires `eval`) with the precompiled static module variant.
3. Remove the `'unsafe-inline'` entry from `scriptSrcAttr` and the conditional `'unsafe-eval'` block in [web-nodejs/middleware/security.js](../../web-nodejs/middleware/security.js#L15-L45).
4. Add an end-to-end test that loads the panel and the remote viewer with a strict CSP enforced and verifies no console violations.

This is a multi-PR effort and is tracked separately in the engineering backlog.
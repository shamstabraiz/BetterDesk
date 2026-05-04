# Phase 2-5 Implementation Status — 2026-04-10

This document tracks the pragmatic scaffolding delivered in this session and the
work that remains. The goal was to establish the **HTTP/DB contracts and UI
plumbing** so frontend, backend and agent work can continue independently.

## ✅ Phase 2 — Live Agent Introspection

### Backend (completed previously)
- `services/bdRelay.js` — `requestFromDevice(deviceId, type, payload)` promise API
  with `pendingRequests` Map + 15 s timeout. Handles `command_response` frames
  from agent to resolve/reject pending promises.
- `routes/devices.routes.js` — 9 proxy endpoints:
  - `GET  /api/devices/:id/services`
  - `GET  /api/devices/:id/processes`
  - `GET  /api/devices/:id/events?limit=N`
  - `GET  /api/devices/:id/activity`
  - `POST /api/devices/:id/files/browse`
  - `POST /api/devices/:id/files/read` (≤ 1 MB)
  - `POST /api/devices/:id/screenshot`
  - `POST /api/devices/:id/terminal/execute` (audit-logged)
  - `POST /api/devices/:id/rename`
- Error mapping: 503 `agent_offline` · 504 `agent_timeout` · 502 other.
- Permission model: `device.view` for reads, `device.edit` for mutations.

### Frontend (completed this session)
- `public/js/deviceDetail.js` — 5 new lazy-loaded tabs (services, processes,
  events, activity, files). Renderers for each data shape. Retry button wired
  to `_loadAgentTab()`. File browser supports parent navigation + double-click
  to descend into directories.
- `public/css/device-detail.css` — full styling for agent tabs (tables, event
  list with severity colors, activity bars, file browser, loading spinner,
  error states).
- `lang/{en,pl,zh}.json` — 15 new keys per language under `device_detail`.

### Agent side (Rust) — **TODO**
Agent client needs a signal WS module that:
1. Connects to `wss://{server}/ws/bd-signal?device_id=X&token=JWT`.
2. Listens for `{type, request_id, payload}` frames.
3. Dispatches to capability handlers and replies with
   `{type: 'command_response', request_id, ok, data?, error?}`.

Recommended Rust modules under `yomie-agent-client/src-tauri/src/`:

| Module             | Responsibility                                          |
| ------------------ | ------------------------------------------------------- |
| `signal_ws.rs`     | tokio-tungstenite client, reconnect, dispatcher         |
| `caps/services.rs` | `Get-Service` / `systemctl list-units --output=json`    |
| `caps/processes.rs`| `sysinfo` crate enumeration, top-by-CPU sort            |
| `caps/events.rs`   | `wevtutil` / `journalctl --output=json`                 |
| `caps/files.rs`    | `std::fs::read_dir` with canonicalize-based sandbox     |
| `caps/terminal.rs` | One-shot `std::process::Command` (no PTY yet)           |
| `caps/screenshot.rs`| `scrap`/`xcap` + `image` crate, base64 PNG/JPEG        |
| `caps/activity.rs` | Background thread tracking process foreground time      |

Deps to add to `src-tauri/Cargo.toml`:
```
tokio-tungstenite = { version = "0.23", features = ["rustls-tls-native-roots"] }
sysinfo = "0.32"          # already present
scrap   = "0.5"           # or xcap
image   = "0.25"
base64  = "0.22"
```

## ✅ Phase 3 — Chat attachments (already done)

- `routes/chat.routes.js` already implements multipart upload (50 MB cap),
  path-traversal-safe download, and search proxy. No additional work needed
  for scaffolding. Contacts/conversations endpoints still **TODO**.

## ✅ Phase 4 — Operator identity + consent popup

### Backend (this session)
- `services/dbAdapter.js` — 6 new `users` columns (`first_name`, `last_name`,
  `email`, `phone`, `role_display`, `avatar_url`) migrated for both SQLite and
  PostgreSQL.
- `services/dbAdapter.js` — `updateUserProfile(id, fields)` helper
  (sanitizes, caps at 200 chars, whitelists allowed keys).
- `routes/phase4_5.routes.js`:
  - `GET  /api/users/me/profile` — current profile
  - `PUT  /api/users/me/profile` — update fields
  - `GET  /api/bd/operator-info?session_id=…` — consent-popup payload

### Agent side — **TODO**
- Rust module `remote/consent.rs`: Tauri window popup with operator identity
  + granular permission checkboxes (keyboard/clipboard/audio/file_transfer/
  recording/camera/block). Countdown optional.
- Session handshake: server sends `remote_request` to agent over signal WS,
  agent shows popup, replies `remote_accept` (with granted permissions) or
  `remote_reject`. Unattended mode skips popup based on `access_policy`.

### Frontend — **TODO**
- Profile-edit page (`/settings/profile`) with avatar upload.

## ✅ Phase 5 — Agent templates + downloads portal

### Backend (this session)
- `routes/phase4_5.routes.js`:
  - `GET    /api/agent-templates`  — list (`enrollment.manage`)
  - `POST   /api/agent-templates`  — create, returns enrollment_token
  - `DELETE /api/agent-templates/:id`
  - `POST   /api/bd/enroll`        — public endpoint for agent enrollment
  - `GET    /portal`               — branded public download page
  - `GET    /api/portal/installers`— installer metadata JSON
- `agent_templates` table auto-created in auth.db on first request
  (`id, name, description, config_json, enrollment_token, created_by,
   created_at, updated_at`).
- `views/downloads-portal.ejs` — standalone glassmorphism download page.

### Frontend — **TODO**
- `/enrollment` admin page with wizard (name, description, capabilities,
  tags, groups → create template → show enrollment token + QR + one-liner
  install command).

### Agent side — **TODO**
- Installer accepts `--enrollment-token=XXXX --server=URL` flags.
- Agent calls `POST /api/bd/enroll` with device_id + sysinfo, receives
  `preset_config`, then proceeds with normal `/api/bd/register`.

## Files touched this session

- `web-nodejs/public/js/deviceDetail.js`
- `web-nodejs/public/css/device-detail.css`
- `web-nodejs/lang/{en,pl,zh}.json`
- `web-nodejs/services/dbAdapter.js` (SQLite + PostgreSQL migrations + profile helper)
- `web-nodejs/routes/index.js` (mount phase4_5)
- `web-nodejs/routes/phase4_5.routes.js` (new)
- `web-nodejs/views/downloads-portal.ejs` (new)
- `tasks/PHASE_2_5_STATUS.md` (this file)

## Deployment notes

1. Restart Node.js console so new routes + migrations load.
2. Migrations are idempotent — safe to re-run.
3. `agent_templates` table is created lazily on first `/api/agent-templates`
   access; no DB downtime required.
4. Agent-client Rust work can begin immediately — HTTP contract is stable.

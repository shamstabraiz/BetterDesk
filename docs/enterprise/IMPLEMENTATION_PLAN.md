# Yomie Console — Implementation Plan v3.0

> **Purpose**: Living document for tracking development priorities, planned features, and community contributions.  
> **Last updated**: 2026-03-22  
> **Status**: Active development — contributions welcome!

---

## How to Contribute

Add your ideas to the **[Ideas & Feature Requests](#ideas--feature-requests)** section at the bottom.
Format: `- [ ] **Feature name** — Brief description (module: X, priority: High/Medium/Low)`

---

## Current State Overview

### Platform Architecture

| Component | Technology | Status |
|-----------|-----------|--------|
| **Go Server** | Go, protobuf, Ed25519, SQLite/PostgreSQL | Production |
| **Web Console** | Node.js, Express, EJS, vanilla JS | Production |
| **CDAP Gateway** | WebSocket, Go server integrated | Production |
| **Yomie Agent** | Go binary, gopsutil, cross-platform | Production |
| **Bridge SDKs** | Python + Node.js CDAP SDKs | Released v1.0.0 |
| **ALL-IN-ONE Scripts** | bash + PowerShell installers | v2.4.0 |
| **Docker** | Single-container + multi-container | Production |

### Module Inventory (10 enterprise modules)

All modules have **real backend logic** — DB tables, REST endpoints, frontend JS.

| Module | Route | View | JS | Backend Features |
|--------|-------|------|----|------------------|
| **Dashboard** | 190 | 203 | 163 | Go API proxy, server probing, bandwidth stats |
| **Devices** | 400+ | 220+ | 400+ | CRUD, folders, tags, kebab menu, responsive |
| **Inventory** | 283 | 100 | 295 | HW+SW inventory upload, telemetry, enrichment |
| **Tickets** | 528 | 158 | 406 | CRUD + comments + attachments + SLA timer |
| **Network** | 339 | 184 | 305 | ICMP ping, TCP, HTTP, polling loop, history |
| **Activity** | 243 | 69 | 191 | Session upload, summaries, top apps |
| **Automation** | 483 | 292 | 368 | Alert rules, commands, agent poll, SMTP |
| **DataGuard** | 275 | 163 | 267 | DLP policies, event ingestion, agent sync |
| **Reports** | 193 | 115 | 290 | 7 report types, CSV export, saved reports |
| **Registration** | 328 | 88 | 303 | LAN discovery, approve/reject, token gen |
| **Tenants** | 285 | 108 | 282 | CRUD, device/user assignment, stats |

**Total enterprise code**: ~8,900 LOC across routes, views, JS, and services.

---

## Priority 1 — Critical Fixes & Polish

> Items that affect daily usability. Should be resolved first.

### 1.1 Alert Engine `periodicCheck()` (Automation)

**Status**: Stub — empty function body  
**Impact**: Alert rules exist in DB but never fire automatically  
**Fix**: Implement periodic evaluation loop in `alertRulesEngine.js`:
- Timer-based check (configurable interval, default 60s)
- Query active rules → evaluate conditions against latest device data
- Trigger actions (email, webhook) when thresholds exceeded
- Respect cooldown periods per rule

**Files**: `services/alertRulesEngine.js`

### 1.2 Tenant Isolation Enforcement

**Status**: Tenants module is standalone CRUD — no cross-module filtering  
**Impact**: Tickets, activity, inventory etc. don't respect tenant boundaries  
**Fix**: Add `tenant_id` filtering middleware for multi-tenant deployments:
- Middleware reads operator's assigned tenant(s)
- All list queries add `WHERE tenant_id IN (...)` clause
- Device-level operations check tenant membership
- Admin bypasses all filters

**Files**: `middleware/tenantFilter.js` (new), all route files

### 1.3 Device Registration — Active Network Scanning

**Status**: Current registration is passive (device must initiate)  
**Impact**: Admin cannot discover unregistered devices on the network  
**Planned features**:
- [ ] Subnet scanner (ICMP sweep + ARP table)
- [ ] Port scanner for RustDesk ports (21116, 21117)
- [ ] Auto-detection of devices running Yomie agent
- [ ] Scheduled scans with configurable subnets
- [ ] Discovery results → pending registration queue
- [ ] Agent-based network neighbor discovery

**Files**: `services/lanDiscovery.js` (extend), `routes/registration.routes.js`

### 1.4 File Transfer Route Exposure

**Status**: `fileTransferService.js` exists (287 LOC) but no HTTP routes  
**Impact**: Feature is backend-only, not accessible from panel  
**Fix**: Create `routes/fileTransfer.routes.js` endpoints + UI integration

---

## Priority 2 — Feature Completion

> Modules that work but need additional features for production use.

### 2.1 Activity Module Enhancement

**Current**: Basic table rendering (view is only 69 LOC)  
**Planned**:
- [ ] Daily/weekly/monthly activity charts (Chart.js)
- [ ] Application usage breakdown (pie/bar charts)
- [ ] Idle time vs. active time comparison
- [ ] Per-user productivity dashboard
- [ ] Activity export to CSV/PDF
- [ ] Session timeline visualization

### 2.2 Network Monitoring Improvements

**Current**: ICMP, TCP, HTTP checks with history  
**Planned**:
- [ ] SNMP v2c/v3 device queries (basic OID polling)
- [ ] Network topology map visualization (D3.js)
- [ ] Bandwidth monitoring per device
- [ ] Alert integration (down → automation alert rule)
- [ ] DNS monitoring
- [ ] SSL certificate expiry checks

### 2.3 Report Engine Extensions

**Current**: 7 report types with CSV export  
**Planned**:
- [ ] PDF generation (pdfmake or puppeteer)
- [ ] Scheduled report generation (node-cron)
- [ ] Email delivery of reports
- [ ] Custom report builder (drag & drop fields)
- [ ] Compliance report templates (GDPR, SOC2)
- [ ] Dashboard export as report

### 2.4 Ticket System Enhancements

**Current**: Full CRUD with SLA, comments, attachments  
**Planned**:
- [ ] Email-to-ticket integration (IMAP polling)
- [ ] Ticket templates (pre-filled forms)
- [ ] Auto-assignment rules (round-robin, skill-based)
- [ ] Knowledge base / FAQ module
- [ ] SLA breach notifications
- [ ] Customer portal (external-facing ticket submission)

### 2.5 Remote Desktop Improvements

**Current**: Web remote client (Beta), web-based RDP via RustDesk protocol  
**Planned**:
- [ ] Multi-monitor selection in web client
- [ ] File transfer during remote session
- [ ] Chat during remote session
- [ ] Session recording (WebM export)
- [ ] Connection quality indicator (latency, FPS)
- [ ] Keyboard shortcut passthrough (Ctrl+Alt+Del, etc.)

---

## Priority 3 — New Capabilities

> Larger features requiring new infrastructure.

### 3.1 Software Deployment

**Description**: Push software installations from panel to devices  
**Components**:
- Package repository (local file store)
- Deployment tasks with targets (device/group/tag)
- Agent-side: download + silent install + report result
- Rollback capability
- Deployment history and success rate

**Files** (new):
```
routes/deployment.routes.js
views/deployment.ejs
public/js/deployment.js
public/css/deployment.css
services/deploymentService.js
```

### 3.2 Patch Management

**Description**: Track and deploy OS/software patches  
**Components**:
- Agent reports installed patches and pending updates
- Admin approves/schedules patch deployment
- Patch compliance dashboard
- Exclusion rules (skip certain patches)

### 3.3 Asset Lifecycle Management

**Description**: Track devices from procurement to disposal  
**Components**:
- Asset status workflow (Ordered → Received → Deployed → Retired)
- Purchase tracking (cost, vendor, warranty dates)
- Warranty expiry alerts
- Location tracking (building/floor/room)
- QR code / barcode label generation
- Assignment history (who had this device when)

### 3.4 User Self-Service Portal

**Description**: End-users can submit tickets and view their devices  
**Components**:
- Separate login page (no admin access)
- Submit tickets with screenshots
- View ticket status and history
- View assigned devices
- Password reset request
- Knowledge base search

### 3.5 Compliance & Audit Dashboard

**Description**: Centralized compliance monitoring  
**Components**:
- Device compliance score (encryption, OS updates, antivirus)
- Policy violation alerts
- Audit log search and export
- Compliance report scheduling
- GDPR data subject access request workflow

---

## Priority 4 — Architecture & Scaling

> Infrastructure improvements for larger deployments.

### 4.1 Real-Time WebSocket Push

**Status**: Currently using polling for status updates  
**Goal**: Push device status, alerts, ticket updates via WebSocket  
**Components**:
- Server-Sent Events or WebSocket endpoint
- Event bus integration (Go server `events/` package)
- Client-side reconnection handling
- Per-module event channels

### 4.2 Plugin / Extension System

**Description**: Allow custom modules without modifying core  
**Components**:
- Plugin manifest format (JSON)
- Route registration API
- View slot system (inject UI into sidebar, device detail, etc.)
- Event hook system (on device register, on ticket create, etc.)
- Plugin marketplace (future)

### 4.3 Multi-Instance / HA

**Description**: Support multiple console instances behind load balancer  
**Components**:
- Session store in PostgreSQL/Redis (not file)
- Shared file storage (S3-compatible)
- PostgreSQL LISTEN/NOTIFY for cross-instance events (already in Go server)
- Health check endpoint for load balancer
- Sticky sessions or stateless auth

### 4.4 Mobile App / PWA

**Description**: Mobile-friendly management interface  
**Components**:
- Progressive Web App (service worker + manifest)
- Push notifications (Web Push API)
- Responsive dashboard optimized for mobile
- Quick actions (approve registration, ack alert)

---

## Technical Debt & Cleanup

> Items that don't add features but improve code quality.

| Item | Description | Priority |
|------|-------------|----------|
| Unit tests for HTTP API | No test coverage on Node.js routes | High |
| Integration tests for PostgreSQL | Requires live PostgreSQL instance | Medium |
| CSS variable consolidation | Some modules still use hardcoded colors | Low |
| View template DRY | Several views duplicate stat-card/modal HTML | Low |
| Activity view expansion | Only 69 LOC — needs charts and detail views | Medium |
| Error handling audit | Some routes lack proper try/catch | Medium |
| API documentation | OpenAPI/Swagger spec for REST endpoints | Low |
| Accessibility audit | WCAG 2.1 compliance check for web panel | Low |

---

## Deployment Checklist (per release)

1. [ ] All changes committed to git
2. [ ] `npm audit --omit=dev` reports 0 vulnerabilities
3. [ ] Go server compiles without warnings
4. [ ] ALL-IN-ONE scripts updated (if new env vars or features)
5. [ ] i18n keys added to EN + PL + ZH
6. [ ] Cache version bumps on deploy (server restart)
7. [ ] Docker images rebuild if Dockerfile changed
8. [ ] CHANGELOG.md updated

---

## Ideas & Feature Requests

> Add your ideas below. Format:  
> `- [ ] **Feature name** — Description (module: X, priority: High/Medium/Low)`

<!-- Add your ideas here -->



---

## Version History

| Date | Change |
|------|--------|
| 2026-03-22 | Initial plan created from ENTERPRISE_ROADMAP.md audit |

---

*See also: [ENTERPRISE_ROADMAP.md](ENTERPRISE_ROADMAP.md) for the original feature specification and technology stack decisions.*

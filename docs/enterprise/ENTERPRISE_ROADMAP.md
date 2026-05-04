# Yomie Enterprise Feature Roadmap

> Analysis based on Axence nVision feature specification mapped onto Yomie
> architecture (Node.js console + Tauri desktop agent + Rust hbbs/hbbr servers).
>
> Last updated: 2026-02-26

---

## Feature Assessment Summary

| Category | Total Features | Implementable | Partial | Not Feasible | Already Done |
|----------|---------------|---------------|---------|-------------|-------------|
| Console / RBAC | 10 | 8 | 2 | 0 | 4 |
| Dashboards & Reports | 10 | 10 | 0 | 0 | 2 |
| NETWORK (NMS) | 10 | 8 | 2 | 0 | 0 |
| INVENTORY (Asset) | 14 | 13 | 1 | 0 | 1 |
| USERS (Activity) | 7 | 6 | 1 | 0 | 0 |
| HELPDESK (ITSM) | 11 | 11 | 0 | 0 | 3 |
| DATAGUARD (DLP) | 6 | 5 | 1 | 0 | 0 |
| SMARTTIME (Time) | 6 | 6 | 0 | 0 | 0 |
| Agent Capabilities | 7 | 7 | 0 | 0 | 3 |
| Automation & Alerts | 8 | 8 | 0 | 0 | 0 |
| **TOTAL** | **89** | **82** | **7** | **0** | **13** |

---

## 1. Console / Admin Panel (10 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 1.1 | Centralized management of all modules | **CAN DO** | Medium | Tab-based module navigation in web console |
| 1.2 | Multi-admin concurrent access | **DONE** | — | Express session + Node.js handles this natively |
| 1.3 | RBAC (roles & permissions) | **DONE** | — | Admin + Operator roles implemented in v2.3.0 |
| 1.4 | Event log / audit trail | **PARTIAL** | Medium | audit_log table exists in dbAdapter; needs UI |
| 1.5 | Configurable dashboard widgets | **CAN DO** | High | SolidJS/React widget grid (react-grid-layout equivalent) |
| 1.6 | Global & per-branch views | **CAN DO** | Medium | Add organization/branch model to DB + filter UI |
| 1.7 | Local + web access | **DONE** | — | Web console on port 5000 + desktop panel planned |
| 1.8 | Periodic & ad-hoc reports | **CAN DO** | High | Report engine with cron scheduling |
| 1.9 | Export PDF / CSV | **CAN DO** | Medium | puppeteer/pdfmake for PDF, json2csv for CSV |
| 1.10 | Compliance / audit reports | **CAN DO** | High | Template-based compliance checks |

---

## 2. Dashboards & Reporting (10 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 2.1 | Infrastructure status dashboard | **CAN DO** | Medium | Real-time device status via WebSocket |
| 2.2 | Alerts & alarms panel | **CAN DO** | Medium | Alert rules engine + notification bell |
| 2.3 | System performance dashboard | **CAN DO** | Medium | CPU/RAM/disk charts from agent telemetry |
| 2.4 | User activity dashboard | **CAN DO** | Medium | Aggregate agent activity data |
| 2.5 | Helpdesk tickets dashboard | **CAN DO** | Low | Ticket count by status/priority widgets |
| 2.6 | License & asset status dashboard | **CAN DO** | Medium | Inventory aggregation |
| 2.7 | Scheduled reports | **CAN DO** | Medium | node-cron + report templates |
| 2.8 | Report scheduling | **CAN DO** | Medium | Email delivery of periodic reports |
| 2.9 | PDF export | **CAN DO** | Medium | pdfmake / puppeteer |
| 2.10 | CSV export | **DONE** | — | Basic CSV export possible now, extend per module |

---

## 3. NETWORK Module — NMS (10 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 3.1 | Auto-discovery of network devices | **CAN DO** | High | ARP scan + mDNS + SNMP walk from agent or server |
| 3.2 | Availability monitoring (PING/TCP/UDP) | **CAN DO** | Medium | Node.js net-ping / raw-socket, or agent-side |
| 3.3 | Service monitoring (HTTP/SMTP/SQL/FTP) | **CAN DO** | Medium | Per-service health checks with configurable endpoints |
| 3.4 | SNMP v1/v2c/v3 | **CAN DO** | High | net-snmp npm package; requires careful v3 auth |
| 3.5 | Device parameter reading (CPU/RAM/temp) | **CAN DO** | Medium | Agent-side: WMI (Windows), /proc (Linux) |
| 3.6 | Syslog / SNMP Trap receiver | **PARTIAL** | High | Syslog UDP listener feasible; SNMP Trap needs low-level |
| 3.7 | Network topology maps | **PARTIAL** | Very High | D3.js/Cytoscape.js visualization; auto-layout complex |
| 3.8 | Threshold-based conditional alerts | **CAN DO** | Medium | Alert rules engine with operators (>, <, ==) |
| 3.9 | Auto-actions (restart, script, notify) | **CAN DO** | Medium | Agent executes command on trigger |
| 3.10 | Performance history & trends | **CAN DO** | Medium | Time-series storage + Chart.js/Plotly graphs |

---

## 4. INVENTORY Module — Asset Management (14 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 4.1 | CPU info (model, clock, cores) | **CAN DO** | Low | sysinfo crate (Rust agent), WMI (Windows) |
| 4.2 | RAM info (capacity, usage) | **CAN DO** | Low | sysinfo crate |
| 4.3 | Disk info (SMART, capacity, usage) | **CAN DO** | Medium | smartctl/WMI for SMART; sysinfo for capacity |
| 4.4 | Network adapters | **CAN DO** | Low | sysinfo + if_addrs crate |
| 4.5 | BIOS / UEFI info | **CAN DO** | Low | WMI Win32_BIOS / dmidecode (Linux) |
| 4.6 | Motherboard info | **CAN DO** | Low | WMI Win32_BaseBoard |
| 4.7 | Peripherals | **PARTIAL** | Medium | USB device enumeration; limited cross-platform |
| 4.8 | Installed applications list | **CAN DO** | Medium | Windows registry scan / dpkg/rpm (Linux) |
| 4.9 | Software versions & publishers | **CAN DO** | Medium | Part of installed apps scan |
| 4.10 | License key detection | **CAN DO** | Medium | Registry keys for known products (partial coverage) |
| 4.11 | License compliance | **CAN DO** | High | Asset DB vs purchased licenses comparison |
| 4.12 | Install/uninstall history | **CAN DO** | Medium | Windows Event Log (MsiInstaller) / agent tracking |
| 4.13 | Offline audit | **CAN DO** | Medium | Agent caches data locally, syncs when online |
| 4.14 | Asset assignment to users | **DONE** | — | Device → user mapping exists in peer table |

---

## 5. USERS Module — Activity Monitoring (7 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 5.1 | Work time registration | **CAN DO** | Medium | Agent tracks login/logout + idle detection |
| 5.2 | Active application monitoring | **CAN DO** | Medium | GetForegroundWindow (Win) / xdotool (Linux) |
| 5.3 | Web browsing monitoring | **CAN DO** | High | Browser history scan or proxy-based; privacy concerns |
| 5.4 | Network transfer analysis | **CAN DO** | Medium | Per-process bandwidth via ETW (Windows) / netfilter |
| 5.5 | Activity classification | **CAN DO** | Medium | Category rules: productive/neutral/unproductive |
| 5.6 | Privacy policies & scope control | **PARTIAL** | Medium | Admin defines which users/groups are monitored |
| 5.7 | Individual & team reports | **CAN DO** | Medium | Aggregated time/activity data with drill-down |

---

## 6. HELPDESK Module — ITSM (11 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 6.1 | User ticket submissions (portal/agent) | **CAN DO** | Medium | Web form + agent tray icon → POST /api/tickets |
| 6.2 | Categories & priorities | **CAN DO** | Low | DB fields + admin config UI |
| 6.3 | SLA & escalation rules | **CAN DO** | High | Time-based triggers, escalation chains |
| 6.4 | Ticket status & history | **CAN DO** | Low | Status workflow + audit trail |
| 6.5 | Attachments & comments | **CAN DO** | Medium | File storage + comment thread per ticket |
| 6.6 | Remote desktop (RDP-like) | **DONE** | — | Core Yomie feature — screen sharing + control |
| 6.7 | Non-disruptive user session access | **DONE** | — | Yomie agent allows shadowing without logoff |
| 6.8 | File transfer | **CAN DO** | Medium | Relay-based file transfer via WebSocket |
| 6.9 | Remote command execution | **CAN DO** | Medium | Agent-side shell exec with output streaming |
| 6.10 | Remote software installation | **CAN DO** | Medium | Push MSI/scripts via agent |
| 6.11 | Knowledge base | **CAN DO** | Medium | Markdown articles with search |
| 6.12 | Email notifications | **CAN DO** | Low | nodemailer integration |
| 6.13 | Work through NAT / Internet | **DONE** | — | WebSocket relay handles NAT traversal |

---

## 7. DATAGUARD Module — DLP (6 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 7.1 | USB device control | **CAN DO** | High | Windows: Group Policy / WMI device control |
| 7.2 | Device whitelist/blacklist | **CAN DO** | Medium | Agent-side policy based on VID/PID/serial |
| 7.3 | Media operation log | **CAN DO** | Medium | File system watcher on removable drives |
| 7.4 | Block data copying | **PARTIAL** | High | Clipboard + file copy hooks; not 100% bulletproof |
| 7.5 | Per-user/group security policies | **CAN DO** | Medium | Policy engine with group inheritance |
| 7.6 | Data operation audit | **CAN DO** | Medium | Detailed logging of file operations to server |

---

## 8. SMARTTIME Module — Time Analytics (6 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 8.1 | Activity time analysis | **CAN DO** | Medium | Derived from Users module data |
| 8.2 | Productivity reports | **CAN DO** | Medium | Productive vs unproductive time ratios |
| 8.3 | Per-user / per-department summaries | **CAN DO** | Medium | Group-by queries + visual reports |
| 8.4 | Productivity rule definitions | **CAN DO** | Medium | Admin UI for app/site → category mapping |
| 8.5 | Charts & statistics | **CAN DO** | Medium | Chart.js / ApexCharts integration |
| 8.6 | Data export | **CAN DO** | Low | CSV / PDF export buttons |

---

## 9. Agent Capabilities (7 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 9.1 | Hardware & software data collection | **CAN DO** | Medium | Rust sysinfo + WMI queries |
| 9.2 | User activity monitoring | **CAN DO** | Medium | Foreground app + idle tracking |
| 9.3 | Remote assistance support | **DONE** | — | Core Yomie remote desktop |
| 9.4 | Admin command execution | **CAN DO** | Medium | Elevated shell via agent service |
| 9.5 | Encrypted communication (AES/TLS) | **DONE** | — | E2E encryption (XSalsa20-Poly1305) + TLS |
| 9.6 | Low CPU/RAM footprint | **DONE** | — | Rust agent is inherently efficient |
| 9.7 | Background operation (no user interaction) | **CAN DO** | Medium | Windows Service + Linux systemd |

---

## 10. Automation & Alerts (8 features)

| # | Feature | Status | Difficulty | Notes |
|---|---------|--------|-----------|-------|
| 10.1 | Conditional alerts (thresholds) | **CAN DO** | Medium | Alert rules engine |
| 10.2 | Time-based alerts | **CAN DO** | Medium | Cron-triggered checks |
| 10.3 | Event-driven alerts | **CAN DO** | Medium | Webhook + agent event stream |
| 10.4 | Email notifications | **CAN DO** | Low | nodemailer / SMTP config |
| 10.5 | SMS notifications | **CAN DO** | Low | Twilio / SMS gateway API |
| 10.6 | System notifications (desktop) | **CAN DO** | Low | Agent-side toast / system tray |
| 10.7 | Script execution on trigger | **CAN DO** | Medium | Agent runs predefined scripts |
| 10.8 | Service/system restart | **CAN DO** | Medium | Agent-side elevated command |

---

## Implementation Phases

### Phase 1 — Core Infrastructure (DONE)
- [x] HTTP registration + WebSocket relay
- [x] E2E encryption
- [x] Database abstraction (SQLite + PostgreSQL)
- [x] RustDesk Client API
- [x] RBAC (admin/operator)
- [x] TOTP 2FA

### Phase 2 — Inventory & Agent Telemetry
**Priority: HIGH | Effort: 3-4 weeks**

New agent module that periodically collects and reports:
- Hardware info (CPU, RAM, disks, network, BIOS, motherboard)
- Installed software list with versions
- OS version, hostname, uptime
- Network configuration (IP, MAC, gateway, DNS)

Server-side:
- `/api/bd/inventory` endpoint to receive inventory data
- `inventory` table in database
- Inventory tab in web console
- Basic search and filter UI

Files to create:
```
yomie-client/src-tauri/src/inventory/
  mod.rs
  hardware.rs        — sysinfo crate integration
  software.rs        — registry/dpkg scan
  collector.rs       — periodic collection + HTTP upload

web-nodejs/routes/inventory.routes.js
web-nodejs/services/inventoryService.js
web-nodejs/views/partials/inventory.ejs (or SolidJS component)
web-nodejs/lang/en.json (inventory keys)
web-nodejs/lang/pl.json (inventory keys)
```

### Phase 3 — Helpdesk / Ticketing System
**Priority: HIGH | Effort: 3-4 weeks**

Ticket system integrated with remote desktop:
- Ticket CRUD (create, assign, resolve, close)
- Categories, priorities, SLA timers
- Agent-side ticket creation (system tray)
- File attachments
- Comment threads
- Email notifications (nodemailer)
- "Connect to device" button on ticket → launches remote session

Files to create:
```
web-nodejs/routes/tickets.routes.js
web-nodejs/services/ticketService.js
web-nodejs/views/partials/tickets.ejs
web-nodejs/migrations/tickets.sql

yomie-client/src-tauri/src/helpdesk/
  mod.rs
  ticket_client.rs   — API client for ticket operations
  tray_integration.rs — system tray "New ticket" menu
```

### Phase 4 — Activity Monitoring & SmartTime
**Priority: MEDIUM | Effort: 4-5 weeks**

Agent tracks foreground application + idle time:
- GetForegroundWindow / xdotool polling (every 5s)
- Idle detection (GetLastInputInfo / X11)
- Activity session aggregation (app name, window title, duration)
- Upload to server every 60s
- Admin-defined productivity categories
- Dashboard with time breakdowns per user/team

Files to create:
```
yomie-client/src-tauri/src/activity/
  mod.rs
  tracker.rs          — foreground app + idle polling
  session.rs          — aggregation logic
  uploader.rs         — periodic HTTP upload

web-nodejs/routes/activity.routes.js
web-nodejs/services/activityService.js
web-nodejs/views/partials/activity-dashboard.ejs
```

### Phase 5 — Network Monitoring (NMS)
**Priority: MEDIUM | Effort: 5-6 weeks**

Server-side network monitoring:
- ICMP ping monitoring (configurable targets)
- TCP port checks
- HTTP endpoint health checks
- SNMP v2c device queries (via net-snmp)
- Alert rules with thresholds
- Performance history (time-series)
- Basic network map visualization (D3.js)

Files to create:
```
web-nodejs/services/networkMonitor.js
web-nodejs/services/snmpService.js
web-nodejs/services/alertEngine.js
web-nodejs/routes/network.routes.js
web-nodejs/views/partials/network-map.ejs
```

### Phase 6 — Automation Engine & Alerts
**Priority: MEDIUM | Effort: 3-4 weeks**

- Alert rules engine (condition → action)
- Email notifications (SMTP config UI)
- Agent-side script execution
- Remote command execution (admin → agent)
- Remote software deployment (push MSI/scripts)
- Scheduled tasks

Files to create:
```
web-nodejs/services/alertRulesEngine.js
web-nodejs/services/emailService.js
web-nodejs/routes/automation.routes.js

yomie-client/src-tauri/src/automation/
  mod.rs
  script_runner.rs
  command_channel.rs  — receive and execute admin commands
```

### Phase 7 — DataGuard (DLP)
**Priority: LOW | Effort: 4-5 weeks**

- USB device enumeration on agent
- Device whitelist/blacklist policies
- Removable media file operation logging
- Clipboard monitoring (optional)
- Policy distribution from server to agent

Files to create:
```
yomie-client/src-tauri/src/dataguard/
  mod.rs
  usb_monitor.rs
  file_watcher.rs
  policy_enforcer.rs

web-nodejs/routes/dataguard.routes.js
web-nodejs/services/dlpService.js
```

### Phase 8 — File Transfer & Advanced Remote
**Priority: MEDIUM | Effort: 2-3 weeks**

- Bidirectional file transfer over WebSocket relay
- Drag-and-drop file send
- Remote file browser
- Remote command shell (interactive terminal)
- Multi-monitor support improvements

Files to create:
```
yomie-client/src-tauri/src/file_transfer/
  mod.rs
  sender.rs
  receiver.rs
  browser.rs

web-nodejs/services/fileTransferService.js
```

### Phase 9 — Reporting Engine
**Priority: LOW | Effort: 3-4 weeks**

- Report template system
- Scheduled report generation (node-cron)
- PDF generation (pdfmake)
- CSV export per module
- Email delivery of reports
- Compliance report templates

### Phase 10 — Multi-Tenancy & Scaling
**Priority: LOW | Effort: 4-5 weeks**

- Organization / branch model
- Per-branch views and filtering
- Tenant isolation in PostgreSQL
- Load balancing relay servers
- Clustered console nodes

---

## Technology Stack Decisions

| Feature Area | Client (Tauri/Rust) | Server (Node.js) |
|-------------|-------------------|------------------|
| HW Inventory | `sysinfo` crate + WMI | dbAdapter storage |
| SW Inventory | Registry scan / dpkg | dbAdapter storage |
| Activity | Win32 API / X11 | aggregation + charts |
| NMS | — (server-only) | `net-ping`, `net-snmp` |
| DLP | Win32 device API | policy distribution |
| Helpdesk | HTTP API client | Express routes + DB |
| File transfer | WebSocket binary frames | relay service |
| Reports | — | `pdfmake`, `json2csv` |
| Email | — | `nodemailer` |
| Charts | — | `Chart.js` / `ApexCharts` |

---

## Estimated Timeline

| Phase | Duration | Cumulative |
|-------|----------|-----------|
| Phase 1 (Core) | — | **DONE** |
| Phase 2 (Inventory) | 3-4 weeks | Month 1 |
| Phase 3 (Helpdesk) | 3-4 weeks | Month 2 |
| Phase 4 (Activity) | 4-5 weeks | Month 3 |
| Phase 5 (NMS) | 5-6 weeks | Month 5 |
| Phase 6 (Automation) | 3-4 weeks | Month 6 |
| Phase 7 (DLP) | 4-5 weeks | Month 7 |
| Phase 8 (File Transfer) | 2-3 weeks | Month 8 |
| Phase 9 (Reporting) | 3-4 weeks | Month 9 |
| Phase 10 (Multi-Tenancy) | 4-5 weeks | Month 10 |

---

## Quick Wins (can be added incrementally to any phase)

- [ ] Email notification service (nodemailer) — reusable across all modules
- [ ] CSV export button on every table view
- [ ] System tray agent menu (Rust: tray-icon crate)
- [ ] Desktop toast notifications (Tauri notification plugin)
- [ ] Dark/light theme persistence (already partially done)

---

*Document auto-generated by analysis of Axence nVision specification against Yomie architecture.*

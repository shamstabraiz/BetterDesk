# CDAP — Connected Device Access Protocol

## Overview

CDAP (Connected Device Access Protocol) is Yomie's unified WebSocket-based protocol for
bidirectional communication between the management server and endpoint agents. It enables:

- **Real-time telemetry** — CPU, memory, disk, and custom metrics streamed continuously
- **Widget-based dashboards** — Agents declare UI widgets (gauges, toggles, buttons, charts) that render in the web console
- **Remote commands** — Operators send structured commands to agents (restart service, run script, toggle relay)
- **File transfer** — Browse, read, write, and delete files on agent endpoints
- **Terminal access** — Full PTY terminal sessions over WebSocket
- **Clipboard sync** — Bidirectional clipboard exchange between browser and device
- **Audio streaming** — PCM/Opus audio relay for voice communication
- **Multi-monitor desktop** — Resolution detection, selective monitor control

## Architecture

```
┌────────────────────┐        WebSocket         ┌──────────────────────┐
│  CDAP Agent        │ ◄═══════════════════════► │  Go Server (:21122)  │
│  (Go / Python /    │    /cdap                  │  cdap/gateway.go     │
│   Node.js binary)  │                           │  cdap/handler.go     │
└────────────────────┘                           └──────────┬───────────┘
                                                            │
                                                   REST API │ + WS proxy
                                                            │
                                                 ┌──────────▼───────────┐
                                                 │  Node.js Console     │
                                                 │  (:5000)             │
                                                 │  routes/cdap.routes  │
                                                 │  cdapMediaProxy.js   │
                                                 └──────────┬───────────┘
                                                            │
                                                   Browser  │ WS
                                                            │
                                                 ┌──────────▼───────────┐
                                                 │  Web Console UI      │
                                                 │  cdap-widgets.js     │
                                                 │  cdap-desktop.js     │
                                                 │  cdap-audio.js       │
                                                 └──────────────────────┘
```

## Message Flow

1. **Agent connects** via WebSocket to `ws://server:21122/cdap`
2. **Authentication** — agent sends `auth` message with API key or device token
3. **Manifest registration** — agent sends `manifest` with device descriptor, capabilities, and widget definitions
4. **Heartbeat loop** — agent sends `heartbeat` every N seconds with system metrics
5. **Widget state** — agent sends `state_update` or `bulk_update` with current widget values
6. **Commands** — server sends `command` messages, agent executes and responds with `command_response`
7. **Events** — server pushes `alert_ack`, `config_update`, `state_request` to agents

## Capability Model

Each agent declares its capabilities in the manifest:

| Capability        | Description                                    |
|-------------------|------------------------------------------------|
| `telemetry`       | System metrics (CPU, RAM, disk, network)       |
| `commands`        | Execute structured commands                    |
| `remote_desktop`  | Screen capture, input injection                |
| `file_transfer`   | File browse, read, write, delete               |
| `clipboard`       | Bidirectional clipboard sync                   |
| `terminal`        | PTY terminal sessions                          |
| `audio`           | Audio streaming (PCM/Opus)                     |
| `screenshot`      | On-demand screenshot capture                   |

## Key Files

| File | Description |
|------|-------------|
| `yomie-server/cdap/gateway.go` | WebSocket gateway, connection management |
| `yomie-server/cdap/handler.go` | Message dispatch, command routing |
| `yomie-server/cdap/audio.go` | Audio session management |
| `yomie-server/cdap/clipboard.go` | Clipboard sync logic |
| `yomie-server/cdap/media_control.go` | Cursor, quality, codec, multi-monitor |
| `yomie-server/api/cdap_handlers.go` | REST + WS HTTP handlers |
| `web-nodejs/routes/cdap.routes.js` | Node.js proxy routes |
| `web-nodejs/services/cdapMediaProxy.js` | WebSocket proxy factory |

See also: [PROTOCOL.md](PROTOCOL.md), [AGENT_GUIDE.md](AGENT_GUIDE.md), [BRIDGE_GUIDE.md](BRIDGE_GUIDE.md)

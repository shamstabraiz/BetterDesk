# Yomie Custom Device API Protocol (CDAP)

> **Status:** RFC / Design Document  
> **Author:** Yomie Team  
> **Created:** 2026-03-19  
> **Version:** 0.2.0 (Draft)  
> **Last Updated:** 2026-03-19

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Problem Statement](#problem-statement)
3. [Architecture Overview](#architecture-overview)
4. [Protocol Design](#protocol-design)
5. [Media Channel Protocol](#media-channel-protocol)
6. [Device Registration & Identity](#device-registration--identity)
7. [Widget Descriptor System](#widget-descriptor-system)
8. [Command Bus](#command-bus)
9. [API Bridge Architecture](#api-bridge-architecture)
10. [Bridge Examples](#bridge-examples)
11. [Security Model](#security-model)
    - [Authentication Layers](#authentication-layers)
    - [Authentication Methods](#authentication-methods)
    - [2FA Integration with Existing System](#2fa-integration-with-existing-system)
    - [Token Lifecycle for Long-Running Agents](#token-lifecycle-for-long-running-agents)
    - [RustDesk Client Synchronization](#rustdesk-client-synchronization)
    - [RBAC for Widgets](#rbac-for-widgets)
12. [Device Revocation Protocol](#device-revocation-protocol)
    - [Problem: Current Gaps](#problem-current-gaps)
    - [Revocation Architecture](#revocation-architecture)
    - [CDAP Revocation Messages](#cdap-revocation-messages)
    - [RustDesk Device Revocation](#rustdesk-device-revocation)
    - [Panel Revocation UI](#panel-revocation-ui)
    - [Cascade Delete with Linked Devices](#cascade-delete-with-linked-devices)
13. [Panel Integration](#panel-integration)
14. [Technology Stack](#technology-stack)
15. [Implementation Phases](#implementation-phases)
16. [Comparison with Alternatives](#comparison-with-alternatives)
17. [FAQ](#faq)

---

## Executive Summary

The **Custom Device API Protocol (CDAP)** extends Yomie into a universal device management platform with **two orthogonal capabilities**:

1. **Control Plane** — Any networked device (industrial controllers, IoT, OS agents) registers, exposes interactive widgets, and receives commands through the web panel via JSON/WebSocket.
2. **Media Plane** — Full remote desktop experience (screen streaming, input forwarding, clipboard, file transfer, audio) via a binary channel — completely independent of the RustDesk protocol.

Together, these planes enable a **native Yomie client** that offers everything RustDesk does and more, while also supporting non-desktop devices (SCADA, IoT) through the same unified protocol.

**Key innovations**:
- Lightweight **API Bridges** translate between existing device protocols (Modbus, OPC-UA, SNMP, REST) and CDAP in real-time (~50-200 LOC).
- **Dual-channel architecture** uses JSON/WebSocket for control (widgets, commands, auth) and binary/WebSocket for media (video, audio, input) — the same connection, negotiated per-device capabilities.
- Devices choose which channels they need: an ESP32 sensor uses only the control plane; a desktop agent uses both; a camera bridge uses control + video-only media.

```
┌──────────────────────────────────────────────────────────────────┐
│                    Yomie Server (Go)                        │
│                                                                  │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐  ┌───────────────┐  │
│  │ Signal   │  │ Relay    │  │ HTTP API  │  │ CDAP Gateway  │  │
│  │ :21116   │  │ :21117   │  │ :21114    │  │ :21122        │  │
│  └────┬─────┘  └────┬─────┘  └─────┬─────┘  └──────┬────────┘  │
│       │              │              │               │            │
│       └──────────────┴──────────────┴───────┬───────┘            │
│                          │                  │                    │
│                    ┌─────┴──────┐    ┌──────┴───────┐           │
│                    │  Device DB │    │  Media Relay │           │
│                    │  (unified) │    │  (binary WS) │           │
│                    └────────────┘    └──────────────┘           │
└──────────────────────────────────────────────────────────────────┘
        │                │                    │
   RustDesk Clients   Web Panel          CDAP Devices
   (desktop/mobile)   (Node.js)          (via bridges + native clients)
                                              │
              ┌───────────────┬───────────────┼───────────────────┐
              │               │               │                   │
        ┌─────┴─────┐  ┌─────┴─────┐  ┌─────┴───────┐   ┌──────┴──────┐
        │ Yomie│  │  Modbus   │  │  OS Agent   │   │   REST      │
        │ Native    │  │  Bridge   │  │  (daemon)   │   │   Bridge    │
        │ Client    │  └─────┬─────┘  └──────┬──────┘   └──────┬──────┘
        │(desktop+  │        │               │                  │
        │ media)    │  ┌─────┴─────┐  ┌──────┴──────┐   ┌──────┴──────┐
        └───────────┘  │ PLC/SCADA │  │ Linux Kernel│   │ IP Camera   │
                       │ Controller│  │ Subsystems  │   │ / NVR       │
                       └───────────┘  └─────────────┘   └─────────────┘
```

---

## Problem Statement

### Current Limitations

Yomie today manages **RustDesk-compatible desktop clients** — Windows, macOS, Linux machines running the RustDesk remote desktop application. This covers one dimension of IT infrastructure: interactive desktop support.

Modern infrastructure management requires visibility into:

| Domain | Examples | Current Yomie Support |
|--------|----------|---------------------------|
| Remote Desktops | Windows, macOS, Linux workstations | ✅ Full (via RustDesk protocol) |
| Native Remote Desktop | Yomie's own client with extended features | ❌ Depends on RustDesk client |
| Industrial Control | PLCs, SCADA HMIs, RTUs | ❌ None |
| IoT/Edge | Sensors, gateways, Raspberry Pi, ESP32 | ❌ None |
| Network Infrastructure | Switches, routers, firewalls | ❌ None |
| OS-Level Management | Kernel parameters, services, packages | ⚠️ Partial (sysinfo only) |
| Custom Applications | In-house monitoring, lab equipment | ❌ None |
| Video Streams | IP cameras, screen capture, RTSP sources | ❌ None |

### The Bridge Insight

Most existing devices **already have management protocols** — Modbus TCP for PLCs, SNMP for network gear, REST APIs for modern appliances, `/proc` and `/sys` for Linux kernels. Requiring vendors to implement CDAP natively is unrealistic.

Instead, CDAP is designed so that **lightweight bridge programs** translate between the device's native protocol and CDAP in real-time:

```
┌─────────────┐     Native Protocol     ┌─────────────┐     CDAP/WebSocket     ┌─────────────┐
│   Device     │ ──────────────────────► │   Bridge     │ ────────────────────► │  Yomie  │
│ (PLC/SCADA)  │ ◄────────────────────── │  (50-200 LOC)│ ◄──────────────────── │   Server     │
└─────────────┘     Modbus/OPC-UA/...   └─────────────┘     JSON over WS       └─────────────┘
```

A bridge is:
- **Tiny**: 50-200 lines in Python, Node.js, Go, Rust, C — anything with WebSocket + native protocol library
- **Stateless**: Server maintains all state; bridge just translates messages
- **Deployable anywhere**: On the device itself, on a gateway, or on a separate machine
- **Writable by anyone**: No Yomie SDK required — just JSON over WebSocket

---

## Architecture Overview

### Component Model

```
┌─────────────────────────────────────────────────────────┐
│                   Yomie Server (Go)                │
│                                                         │
│  ┌─────────────────────────────────────────────────┐   │
│  │              CDAP Gateway (:21122)               │   │
│  │                                                   │   │
│  │  ┌──────────┐  ┌──────────┐  ┌───────────────┐  │   │
│  │  │ Auth     │  │ Manifest │  │ Command       │  │   │
│  │  │ Handler  │  │ Registry │  │ Router        │  │   │
│  │  └──────────┘  └──────────┘  └───────────────┘  │   │
│  │                                                   │   │
│  │  ┌──────────┐  ┌──────────┐  ┌───────────────┐  │   │
│  │  │ Widget   │  │ Heartbeat│  │ Event         │  │   │
│  │  │ State    │  │ Manager  │  │ Emitter       │  │   │
│  │  └──────────┘  └──────────┘  └───────────────┘  │   │
│  └─────────────────────────────────────────────────┘   │
│                          │                              │
│  ┌───────────┐    ┌──────┴──────┐    ┌──────────────┐  │
│  │ peers DB  │◄──►│ cdap_devices│◄──►│ widget_states│  │
│  │ (unified) │    │    table    │    │    table     │  │
│  └───────────┘    └─────────────┘    └──────────────┘  │
│                          │                              │
│  ┌──────────────────────┐│┌────────────────────────┐   │
│  │   REST API (:21114)  │││  Event Bus (WebSocket) │   │
│  │   /api/cdap/*        │││  cdap.* events         │   │
│  └──────────────────────┘│└────────────────────────┘   │
│                          │                              │
└──────────────────────────┼──────────────────────────────┘
                           │
              ┌────────────┼────────────┐
              │            │            │
         Web Panel    CDAP Bridges   RustDesk Clients
         (Node.js)   (any language)  (desktop)
```

### Data Flow

1. **Registration**: Bridge connects via WebSocket to `:21122`, authenticates with API key, sends device manifest
2. **ID Assignment**: Server generates unique device ID (format: `CDAP-XXXXXXXX`), stores in unified `peers` table with `device_type` field
3. **Widget Sync**: Server parses manifest widgets, creates `widget_states` entries, pushes to panel via event bus
4. **Heartbeat**: Bridge sends periodic heartbeat with live widget values (CPU gauge, pressure reading, etc.)
5. **Commands**: Panel user clicks widget → server validates RBAC → routes command to bridge via WebSocket → bridge translates to native protocol → device responds → bridge sends state update → server pushes to panel

---

## Protocol Design

### Transport

| Property | Value |
|----------|-------|
| **Transport** | WebSocket (RFC 6455) over TCP |
| **Port** | 21122 (configurable via `CDAP_PORT` env var) |
| **Encoding** | JSON (UTF-8) |
| **TLS** | Optional — auto-detected via `DualModeListener` (same as signal/relay) |
| **Keepalive** | WebSocket ping/pong every 30 seconds |
| **Reconnect** | Client-side exponential backoff (1s, 2s, 4s, 8s, max 60s) |
| **Max message size** | 1 MB (configurable via `CDAP_MAX_MESSAGE_SIZE`) |

### Why WebSocket + JSON (not Protobuf/gRPC)

| Criterion | WebSocket + JSON | gRPC/Protobuf |
|-----------|-----------------|---------------|
| Bridge implementation effort | ~10 lines for WS connect | Protobuf codegen + gRPC runtime |
| Language support | Every language has WS libs | Limited on embedded/microcontrollers |
| Debugging | Human-readable, curl-testable | Binary, needs grpcurl |
| Browser compatibility | Native WebSocket API | Requires grpc-web proxy |
| Bidirectional streaming | Native | Native |
| Performance | Sufficient for control plane (<1000 msg/s) | Better for high-throughput |
| Embedded devices (ESP32) | Arduino WebSocket library available | No gRPC runtime for Arduino |

**Decision**: JSON over WebSocket for v1. Binary encoding (MessagePack/CBOR) as optional optimization in v2 for high-frequency telemetry.

### Message Envelope

Every message follows this envelope:

```json
{
  "type": "string",
  "id": "string (optional, for request/response correlation)",
  "timestamp": "ISO-8601",
  "payload": {}
}
```

### Message Types

#### Client → Server

| Type | Description | Payload |
|------|-------------|---------|
| `auth` | Authentication | `{api_key, bridge_version}` |
| `register` | Device manifest | `{manifest}` (see Manifest section) |
| `heartbeat` | Periodic health + telemetry | `{metrics: {}, widget_values: {}}` |
| `state_update` | Widget state change (device-initiated) | `{widget_id, value, timestamp}` |
| `bulk_update` | Multiple widget updates at once | `{updates: [{widget_id, value}]}` |
| `event` | Custom event from device | `{event_type, data}` |
| `command_response` | Response to server command | `{command_id, status, result}` |
| `log` | Device log entry | `{level, message, context}` |
| `unregister` | Graceful disconnect | `{reason}` |

#### Server → Client

| Type | Description | Payload |
|------|-------------|---------|
| `auth_result` | Authentication response | `{success, device_id, session_token}` |
| `registered` | Registration confirmed | `{device_id, server_time}` |
| `command` | Execute action on device | `{command_id, widget_id, action, value}` |
| `config_update` | Server pushes config change | `{key, value}` |
| `ping` | Health check (beyond WS ping) | `{server_time}` |
| `error` | Error notification | `{code, message, details}` |

### Example Session

```
Bridge                              Server
  │                                    │
  │──── auth ─────────────────────────►│
  │     {api_key: "abc123",            │
  │      bridge_version: "1.0.0"}      │
  │                                    │
  │◄─── auth_result ──────────────────│
  │     {success: true,                │
  │      device_id: "CDAP-A7F3B210",  │
  │      session_token: "jwt..."}      │
  │                                    │
  │──── register ─────────────────────►│
  │     {manifest: {...}}              │
  │                                    │
  │◄─── registered ───────────────────│
  │     {device_id: "CDAP-A7F3B210"}  │
  │                                    │
  │──── heartbeat ────────────────────►│  (every 15s)
  │     {widget_values: {              │
  │       "pressure": 3.7,            │
  │       "valve_1": true             │
  │     }}                             │
  │                                    │
  │         (operator clicks widget)   │
  │◄─── command ──────────────────────│
  │     {command_id: "cmd-001",        │
  │      widget_id: "valve_1",         │
  │      action: "set",                │
  │      value: false}                 │
  │                                    │
  │──── command_response ─────────────►│
  │     {command_id: "cmd-001",        │
  │      status: "ok",                 │
  │      result: {valve_1: false}}     │
  │                                    │
  │──── state_update ─────────────────►│
  │     {widget_id: "valve_1",         │
  │      value: false}                 │
  │                                    │
```

---

## Media Channel Protocol

### Overview — Dual-Channel Architecture

CDAP operates on **two channels** over the same WebSocket connection:

| Channel | Encoding | Purpose | Bandwidth | Required |
|---------|----------|---------|-----------|----------|
| **Control** | JSON text frames | Auth, manifest, widgets, commands, heartbeat | Low (~1-10 KB/s) | Always |
| **Media** | Binary frames | Video, audio, input events, clipboard, file transfer | High (~0.1-20 MB/s) | Optional |

A device declares media capabilities in its manifest. If no media capabilities are declared, only the control channel is active (IoT/SCADA mode). If `remote_desktop`, `video_stream`, or `audio` capabilities are declared, the media channel is negotiated after registration.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Single WebSocket Connection                   │
│                                                                  │
│  ┌─────────────────────────────┐  ┌────────────────────────────┐│
│  │     Control Channel         │  │     Media Channel          ││
│  │     (JSON text frames)      │  │     (binary frames)        ││
│  │                             │  │                            ││
│  │  • auth / register          │  │  • video frames (H.264/VP9)││
│  │  • heartbeat / state_update │  │  • audio packets (Opus)    ││
│  │  • command / command_resp   │  │  • input events (kbd/mouse)││
│  │  • widget values            │  │  • clipboard data          ││
│  │  • alerts / logs            │  │  • file transfer chunks    ││
│  │                             │  │  • cursor images           ││
│  └─────────────────────────────┘  └────────────────────────────┘│
│                                                                  │
│  Multiplexing: text frames = control, binary frames = media      │
│  Encryption: XSalsa20-Poly1305 (per-frame, counter-based nonce) │
└─────────────────────────────────────────────────────────────────┘
```

This design means:
- **IoT/SCADA bridges** use only text frames — zero media overhead
- **Desktop agents** use both channels — full remote desktop
- **Camera bridges** use control + video-only media — no input needed
- **One WebSocket, one port, one auth** — no separate media negotiation

### Why Not WebRTC?

| Criterion | CDAP Media (Binary WS) | WebRTC |
|-----------|----------------------|--------|
| NAT traversal | Server relays (existing infra) | STUN/TURN servers (extra infra) |
| Browser support | WebSocket API (universal) | Partial (no Safari iOS Workers) |
| Server-side relay | Native (Go relay already exists) | Requires TURN server |
| Encryption | XSalsa20-Poly1305 (proven, simple) | DTLS-SRTP (complex, more attack surface) |
| Codec negotiation | Manifest-declared (simple) | SDP offer/answer (complex) |
| Firewall friendly | Single port (21122) | Multiple UDP ports, STUN binding |
| Embedded devices | Any WS library | No WebRTC on ESP32/microcontrollers |
| P2P option | Future (hole punch via signal) | Native (ICE candidates) |
| Latency | ~1-5ms added by relay | ~0ms P2P, ~10-50ms via TURN |

**Decision**: Binary WebSocket frames over existing server relay for v1. P2P optimization (UDP hole-punch via signal server) as v2 option for latency-sensitive desktop use.

### Media Frame Format

All media data is sent as WebSocket **binary frames** with a thin header:

```
┌──────────────────────────────────────────────────────┐
│                 CDAP Media Frame                      │
├────────┬───────┬──────────┬──────────────────────────┤
│ Channel│ Flags │ Sequence │         Payload           │
│ (1B)   │ (1B)  │  (4B LE) │  (variable length)       │
├────────┼───────┼──────────┼──────────────────────────┤
│  0x01  │ 0x00  │ 00000001 │ [encrypted payload]      │
└────────┴───────┴──────────┴──────────────────────────┘
```

**Header (6 bytes)**:

| Field | Size | Description |
|-------|------|-------------|
| `channel` | 1 byte | Media sub-channel identifier |
| `flags` | 1 byte | Per-channel flags (keyframe, encrypted, etc.) |
| `sequence` | 4 bytes LE | Monotonic counter for ordering + dedup |

**Channel IDs**:

| ID | Channel | Direction | Description |
|----|---------|-----------|-------------|
| `0x01` | Video | Device → Server/Viewer | Encoded video frames |
| `0x02` | Audio | Bidirectional | Opus audio packets |
| `0x03` | Input | Viewer → Device | Keyboard + mouse events |
| `0x04` | Clipboard | Bidirectional | Clipboard content sync |
| `0x05` | File | Bidirectional | File transfer chunks |
| `0x06` | Cursor | Device → Viewer | Cursor image + position |
| `0x07` | Display | Device → Server | Display info (resolution, monitors) |
| `0x08` | Control | Bidirectional | Media session control messages |
| `0x10-0xFF` | Custom | Bidirectional | Reserved for bridge-defined channels |

**Flags (per channel)**:

| Bit | Channel | Meaning |
|-----|---------|---------|
| `0x01` | Video | Keyframe |
| `0x02` | Video | Display index in first 2 payload bytes |
| `0x04` | File | Last chunk (EOF) |
| `0x08` | All | Encrypted (XSalsa20-Poly1305) |
| `0x10` | All | Compressed (zstd) |

### Capability Declaration

Devices declare media capabilities in the manifest:

```json
{
  "capabilities": [
    "telemetry",
    "commands",
    "remote_desktop",
    "video_stream",
    "audio",
    "clipboard",
    "file_transfer",
    "input_control"
  ],
  "media": {
    "video": {
      "codecs": ["h264", "vp9", "av1"],
      "preferred": "h264",
      "max_resolution": "3840x2160",
      "max_fps": 60,
      "hardware_encoder": true,
      "displays": [
        {"id": 0, "name": "Primary", "width": 1920, "height": 1080},
        {"id": 1, "name": "Secondary", "width": 2560, "height": 1440}
      ]
    },
    "audio": {
      "codecs": ["opus"],
      "sample_rate": 48000,
      "channels": 2,
      "bidirectional": true
    },
    "input": {
      "keyboard": true,
      "mouse": true,
      "touch": false,
      "gamepad": false
    },
    "clipboard": {
      "text": true,
      "image": true,
      "max_size": 10485760
    },
    "file_transfer": {
      "max_file_size": 4294967296,
      "resumable": true
    }
  }
}
```

### Media Session Establishment

After device registration, a viewer (web panel or native client) requests a media session:

```
Viewer                    Server                     Device (Bridge/Agent)
  │                          │                            │
  │── POST /api/cdap/        │                            │
  │   devices/{id}/connect ─►│                            │
  │                          │                            │
  │◄── {session_id,          │                            │
  │     relay_token,         │                            │
  │     device_capabilities} │                            │
  │                          │                            │
  │── WS /ws/cdap/media ────►│                            │
  │   {session_id,           │                            │
  │    relay_token}          │                            │
  │                          │── command ────────────────►│
  │                          │   {action:"media_connect", │
  │                          │    session_id, viewer_pk}  │
  │                          │                            │
  │                          │◄── media_accept ──────────│
  │                          │    {device_pk}             │
  │                          │                            │
  │     ┌────────────────────┤                            │
  │     │ E2E Key Exchange   │                            │
  │     │ (X25519 → XSalsa20)│                            │
  │     └────────────────────┤                            │
  │                          │                            │
  │◄═══ binary: video ══════╪═══════════════════════════│
  │◄═══ binary: audio ══════╪═══════════════════════════│
  │◄═══ binary: cursor ═════╪═══════════════════════════│
  │═══► binary: input ══════╪══════════════════════════►│
  │◄══► binary: clipboard ══╪══════════════════════════►│
  │◄══► binary: file ═══════╪══════════════════════════►│
  │                          │                            │
```

### Video Protocol

#### Frame Payload (channel 0x01)

```
┌─────────────────────────────────────────────┐
│              Video Frame Payload             │
├──────────┬────────┬─────────┬───────────────┤
│ Codec ID │  PTS   │ Display │  Coded Data   │
│  (1B)    │ (8B LE)│  (1B)   │  (variable)   │
└──────────┴────────┴─────────┴───────────────┘
```

| Field | Size | Description |
|-------|------|-------------|
| `codec_id` | 1 byte | `0x01`=VP9, `0x02`=H.264, `0x03`=H.265, `0x04`=VP8, `0x05`=AV1 |
| `pts` | 8 bytes LE | Presentation timestamp (microseconds since session start) |
| `display` | 1 byte | Display/monitor index (0-255) |
| `coded_data` | variable | Codec bitstream bytes |

**Codec Negotiation**: Server sends viewer's supported codecs to device in `media_connect`. Device chooses the best codec from intersection and sends first keyframe. Codec switch mid-session is supported (send new keyframe with different `codec_id`).

**Keyframe Request**: Viewer sends control frame `{action: "request_keyframe", display: 0}` when decoder errors or viewer joins mid-stream.

#### Adaptive Quality

Device-side encoder adjusts quality based on `video_received` acknowledgments from viewer:

```json
// Control channel (0x08): viewer → device
{
  "action": "video_ack",
  "sequence": 42567,
  "decode_time_ms": 3,
  "render_time_ms": 1,
  "buffer_ms": 50
}
```

Device uses ack timing to estimate RTT and adjusts bitrate/resolution/fps dynamically.

### Audio Protocol

#### Packet Payload (channel 0x02)

```
┌──────────────────────────────────────┐
│          Audio Packet Payload         │
├──────────┬─────────┬─────────────────┤
│ Codec ID │   PTS   │  Opus Packet    │
│  (1B)    │ (8B LE) │  (variable)     │
└──────────┴─────────┴─────────────────┘
```

- Codec: Opus (48 kHz, stereo, 20ms frames = 960 samples/frame)
- Bidirectional: device captures system audio → viewer speakers; viewer microphone → device speakers
- Jitter buffer: 3-5 frames (60-100ms) on receiver side

### Input Protocol

#### Event Payload (channel 0x03)

```
┌──────────────────────────────────────────────┐
│            Input Event Payload                │
├──────────┬───────────────────────────────────┤
│ Type     │        Event Data                 │
│  (1B)    │        (variable)                 │
└──────────┴───────────────────────────────────┘
```

**Input Types**:

| Type | ID | Payload |
|------|----|---------|
| Key Down | `0x01` | `{scancode(2B), unicode(4B), modifiers(1B)}` |
| Key Up | `0x02` | `{scancode(2B), unicode(4B), modifiers(1B)}` |
| Mouse Move | `0x03` | `{x(4B LE), y(4B LE), display(1B)}` — absolute position |
| Mouse Button | `0x04` | `{button(1B), pressed(1B), x(4B), y(4B)}` |
| Mouse Wheel | `0x05` | `{delta_x(4B LE signed), delta_y(4B LE signed)}` |
| Touch | `0x06` | `{touch_id(2B), action(1B), x(4B), y(4B), pressure(2B)}` |
| Ctrl+Alt+Del | `0x10` | (no payload — special secure action) |

**Modifier Flags** (1 byte bitmask):

| Bit | Modifier |
|-----|----------|
| `0x01` | Shift |
| `0x02` | Ctrl |
| `0x04` | Alt |
| `0x08` | Meta/Super |
| `0x10` | CapsLock active |
| `0x20` | NumLock active |

### Clipboard Protocol (channel 0x04)

```json
// Clipboard content sent as binary frame with JSON header
{
  "format": "text/plain",
  "size": 1234,
  "hash": "sha256:abc123..."
}
// Followed by raw clipboard bytes
```

Supported formats: `text/plain`, `text/html`, `image/png`, `image/bmp`, `application/x-file-list`.

Max clipboard size configurable (default 10 MB). Hash-based dedup prevents re-sending identical content.

### File Transfer Protocol (channel 0x05)

```
┌──────────────────────────────────────────────────────────┐
│                File Transfer Chunk                        │
├──────────┬────────────┬──────────┬────────┬──────────────┤
│ Transfer │   Offset   │  Total   │ Flags  │   Data       │
│  ID (4B) │  (8B LE)   │  (8B LE) │ (1B)   │  (variable)  │
└──────────┴────────────┴──────────┴────────┴──────────────┘
```

- **Transfer initiation**: JSON message on control channel with filename, size, hash
- **Data transfer**: Binary chunks on media channel (64 KB default chunk size)
- **Resumable**: Client tracks received offsets, can resume from last chunk
- **Bidirectional**: Both viewer→device and device→viewer
- **Multiple simultaneous**: Transfer ID distinguishes parallel transfers

### Cursor Protocol (channel 0x06)

```
┌────────────────────────────────────────────────┐
│              Cursor Update                      │
├────────┬────────┬───────┬───────┬──────────────┤
│ Hot X  │ Hot Y  │ Width │ Height│  RGBA pixels  │
│ (2B LE)│ (2B LE)│(2B LE)│(2B LE)│ (W*H*4 bytes)│
└────────┴────────┴───────┴───────┴──────────────┘
```

- Sent when cursor image changes (not every move — position is in mouse events)
- Hot X/Y: cursor click point offset
- RGBA pixels: uncompressed cursor image (typically small, 32x32 to 128x128)
- Optionally zstd-compressed (flag `0x10`)

### End-to-End Encryption

Media channel encryption is **mandatory** for `remote_desktop` capability and **optional** for other media types:

```
Key Exchange (during media session setup):
  1. Viewer generates ephemeral X25519 keypair
  2. Viewer sends public key to device (via control channel, relay through server)
  3. Device generates ephemeral X25519 keypair
  4. Device sends public key to viewer
  5. Both compute shared secret: X25519(my_secret, their_public)
  6. Derive XSalsa20-Poly1305 key from shared secret (HKDF-SHA256)

Per-Frame Encryption:
  - Nonce: 24 bytes = channel_id(1B) + direction(1B) + sequence(4B) + zeros(18B)
  - Encrypt: XSalsa20-Poly1305(key, nonce, plaintext)
  - Output: 16-byte MAC + ciphertext
  - Sequence counter prevents replay attacks
```

The server **cannot** decrypt media traffic — it only relays binary frames between viewer and device. True end-to-end encryption.

### Native Yomie Client vs RustDesk

CDAP with media channel enables building a **fully native Yomie client** that replaces RustDesk:

| Feature | RustDesk Client | Yomie Native Client (CDAP) |
|---------|----------------|-------------------------------|
| Protocol | RustDesk proprietary protobuf | CDAP (open, documented) |
| Server dependency | Requires hbbs/hbbr compatible server | Yomie server only |
| Video codecs | VP9, H.264, H.265, VP8, AV1 | Same + extensible via manifest |
| Audio | Opus | Opus + extensible |
| E2E encryption | NaCl secretbox | XSalsa20-Poly1305 (compatible) |
| Widgets/controls | None (pure remote desktop) | Full widget system alongside remote desktop |
| File transfer | Built-in (proprietary) | CDAP file channel (documented, extensible) |
| Multi-monitor | Yes | Yes (display index in manifest) |
| Session recording | No | Server-side recording support (planned) |
| OS-level management | No | Yes (via widgets: services, packages, shell) |
| Custom actions | No | Yes (bridge-defined buttons, toggles) |
| Mixed-mode device | No | Yes — remote desktop + SCADA widgets on same device |
| Browser viewer | Web client (WIP) | Native CDAP player in panel (same tech) |
| Unattended access | Separate password | Same auth (JWT/API key) |
| Update mechanism | Manual | Server-pushed config updates |
| Plugin system | None | Bridge/manifest-based extensibility |

**Key advantage**: A Yomie native client is both a remote desktop tool AND a management agent. A single install gives operators remote screen access, system monitoring widgets, remote shell, file transfer, and custom integrations — all through the same protocol and panel.

### Backward Compatibility with RustDesk Ecosystem

CDAP does **not** replace RustDesk protocol support in Yomie server. Both protocols coexist:

```
┌──────────────────────────────────────────────────────┐
│                  Yomie Server                     │
│                                                        │
│  ┌──────────────────┐    ┌───────────────────────┐   │
│  │ RustDesk Protocol │    │   CDAP Protocol       │   │
│  │                    │    │                       │   │
│  │ • Signal :21116   │    │ • Gateway :21122      │   │
│  │ • Relay  :21117   │    │ • Media relay (same)  │   │
│  │ • API    :21114   │    │ • REST /api/cdap/*    │   │
│  │                    │    │                       │   │
│  │ For: existing      │    │ For: new native       │   │
│  │ RustDesk clients   │    │ clients, IoT, SCADA   │   │
│  └────────┬───────────┘    └──────────┬────────────┘   │
│           │                           │                │
│           └───────────┬───────────────┘                │
│                       │                                │
│              ┌────────┴────────┐                       │
│              │  Unified peers  │                       │
│              │  table + panel  │                       │
│              └─────────────────┘                       │
└──────────────────────────────────────────────────────┘
```

Migration path:
1. **Phase 1**: Existing RustDesk clients continue working unchanged
2. **Phase 2**: Yomie native client available as alternative with extra features
3. **Phase 3**: Users can gradually migrate devices from RustDesk protocol to CDAP
4. **Long term**: CDAP becomes the primary protocol; RustDesk support maintained for backward compatibility

---

## Device Registration & Identity

### Device Manifest

The manifest is the core descriptor — it tells the server everything about the device and how to render its control interface.

```json
{
  "manifest_version": "1.0",
  "device": {
    "name": "Boiler Room Controller",
    "type": "scada",
    "vendor": "Siemens",
    "model": "S7-1200",
    "firmware": "4.5.2",
    "serial": "SN-2024-00847",
    "location": "Building A, Floor 2, Room 201",
    "tags": ["boiler", "hvac", "critical"],
    "icon": "factory",
    "description": "Main boiler room PLC controlling 3 gas boilers and circulation pumps"
  },
  "bridge": {
    "name": "modbus-yomie-bridge",
    "version": "1.2.0",
    "protocol": "modbus-tcp",
    "target_host": "192.168.10.50",
    "target_port": 502
  },
  "capabilities": [
    "telemetry",
    "commands",
    "alerts",
    "logs"
  ],
  "heartbeat_interval": 15,
  "widgets": [
    // see Widget Descriptor System section
  ],
  "alerts": [
    {
      "id": "high_pressure",
      "label": "High Pressure Alarm",
      "severity": "critical",
      "condition": "pressure > 8.0",
      "message": "Boiler pressure exceeded 8.0 bar"
    },
    {
      "id": "pump_failure",
      "label": "Pump Failure",
      "severity": "warning",
      "condition": "pump_1_status == false && pump_1_expected == true",
      "message": "Circulation pump 1 stopped unexpectedly"
    }
  ]
}
```

### Device Types

| Type Identifier | Display Name | Icon | Description |
|----------------|--------------|------|-------------|
| `rustdesk` | Remote Desktop | `monitor` | Standard RustDesk client (existing) |
| `scada` | SCADA/PLC | `factory` | Industrial controller, PLC, HMI |
| `iot` | IoT Device | `cpu` | Sensor, gateway, embedded system |
| `os_agent` | OS Agent | `terminal` | OS-level management daemon |
| `network` | Network Device | `globe` | Switch, router, firewall, AP |
| `camera` | Camera/NVR | `video` | IP camera, NVR, DVR |
| `desktop` | Yomie Desktop | `monitor-smartphone` | Native Yomie client (remote desktop + agent) |
| `custom` | Custom Device | `puzzle` | User-defined type |

### ID Format

- RustDesk devices: numeric IDs (e.g., `1340238749`) — unchanged
- CDAP devices: `CDAP-` prefix + 8-char hex (e.g., `CDAP-A7F3B210`)
- ID persisted across reconnects (server matches by `bridge.serial` or `device.serial`)
- Manual ID override: bridge can request specific ID in manifest (`requested_id` field)

### Unified Device List

CDAP devices are stored in the same `peers` table as RustDesk devices, with additional fields:

```sql
ALTER TABLE peers ADD COLUMN device_type TEXT DEFAULT 'rustdesk';
ALTER TABLE peers ADD COLUMN manifest_json TEXT;
ALTER TABLE peers ADD COLUMN bridge_id TEXT;
ALTER TABLE peers ADD COLUMN cdap_session_id TEXT;
```

This means:
- Dashboard counters include all device types
- Device list supports filtering by type
- Search works across all devices
- Tags, notes, user assignment — all work identically
- Ban/soft-delete applies to CDAP devices too

---

## Widget Descriptor System

Widgets are the UI building blocks that a device exposes to the panel. The bridge declares widgets in the manifest; the panel renders them dynamically.

### Widget Types

#### `toggle` — On/Off Switch

```json
{
  "type": "toggle",
  "id": "valve_main",
  "label": "Main Valve",
  "group": "Valves",
  "value": true,
  "readonly": false,
  "confirm": true,
  "confirm_message": "Are you sure you want to toggle the main valve?"
}
```

#### `gauge` — Numeric Value with Range

```json
{
  "type": "gauge",
  "id": "pressure",
  "label": "Boiler Pressure",
  "group": "Sensors",
  "value": 3.7,
  "unit": "bar",
  "min": 0,
  "max": 10,
  "warning_low": 1.0,
  "warning_high": 7.0,
  "critical_low": 0.5,
  "critical_high": 8.5,
  "precision": 1,
  "readonly": true
}
```

#### `button` — Action Trigger

```json
{
  "type": "button",
  "id": "emergency_stop",
  "label": "Emergency Stop",
  "group": "Safety",
  "style": "danger",
  "confirm": true,
  "confirm_message": "This will immediately shut down all boilers. Confirm?",
  "icon": "alert-octagon",
  "cooldown": 5
}
```

#### `chart` — Time-Series Graph

```json
{
  "type": "chart",
  "id": "temperature_history",
  "label": "Temperature Trend",
  "group": "Monitoring",
  "chart_type": "line",
  "points": 100,
  "unit": "°C",
  "min": 0,
  "max": 200,
  "series": [
    {"id": "temp_supply", "label": "Supply", "color": "#ef4444"},
    {"id": "temp_return", "label": "Return", "color": "#3b82f6"}
  ],
  "retention": "24h"
}
```

#### `select` — Dropdown/Mode Selector

```json
{
  "type": "select",
  "id": "operating_mode",
  "label": "Operating Mode",
  "group": "Control",
  "value": "auto",
  "options": [
    {"value": "auto", "label": "Automatic"},
    {"value": "manual", "label": "Manual"},
    {"value": "standby", "label": "Standby"},
    {"value": "maintenance", "label": "Maintenance"}
  ],
  "readonly": false
}
```

#### `slider` — Numeric Input with Range

```json
{
  "type": "slider",
  "id": "setpoint_temp",
  "label": "Temperature Setpoint",
  "group": "Control",
  "value": 75,
  "min": 40,
  "max": 95,
  "step": 1,
  "unit": "°C",
  "readonly": false
}
```

#### `text` — Read-Only Text Display

```json
{
  "type": "text",
  "id": "last_error",
  "label": "Last Error",
  "group": "Diagnostics",
  "value": "E104: Flame sensor timeout at 14:23:07",
  "style": "error"
}
```

#### `table` — Tabular Data

```json
{
  "type": "table",
  "id": "process_list",
  "label": "Running Processes",
  "group": "System",
  "columns": [
    {"id": "pid", "label": "PID", "width": "80px"},
    {"id": "name", "label": "Name"},
    {"id": "cpu", "label": "CPU %", "width": "100px"},
    {"id": "memory", "label": "Memory", "width": "100px"}
  ],
  "max_rows": 50,
  "sortable": true,
  "readonly": true
}
```

#### `led` — Status Indicator

```json
{
  "type": "led",
  "id": "pump_1_status",
  "label": "Pump 1",
  "group": "Status",
  "value": "green",
  "states": {
    "green": "Running",
    "yellow": "Starting",
    "red": "Fault",
    "gray": "Offline"
  }
}
```

#### `terminal` — Command Shell (OS Agent)

```json
{
  "type": "terminal",
  "id": "shell",
  "label": "Remote Shell",
  "group": "Management",
  "shell": "/bin/bash",
  "max_history": 1000,
  "allowed_commands": ["systemctl", "journalctl", "ip", "ss", "df", "free"],
  "blocked_commands": ["rm", "dd", "mkfs", "reboot"]
}
```

#### `desktop` — Remote Desktop Viewer (Media Channel)

```json
{
  "type": "desktop",
  "id": "remote_screen",
  "label": "Remote Desktop",
  "group": "Remote Access",
  "display": 0,
  "codec": "auto",
  "max_fps": 60,
  "audio": true,
  "clipboard": true,
  "file_transfer": true,
  "fullscreen": true
}
```

Requires `remote_desktop` capability in manifest. Opens a media channel session on click.
Panel renders an interactive Canvas/WebCodecs viewer with input forwarding, clipboard sync, and file transfer toolbar.

#### `video_stream` — One-Way Video (Media Channel)

```json
{
  "type": "video_stream",
  "id": "camera_feed",
  "label": "Lobby Camera",
  "group": "Surveillance",
  "display": 0,
  "codec": "h264",
  "max_fps": 30,
  "audio": true,
  "controls": ["snapshot", "record"]
}
```

One-directional video — no input forwarding. Suitable for IP cameras, NVR feeds, kiosk displays.

#### `file_browser` — File Transfer UI

```json
{
  "type": "file_browser",
  "id": "files",
  "label": "File Manager",
  "group": "Management",
  "root_paths": ["/home", "/var/log", "/etc"],
  "upload": true,
  "download": true,
  "delete": false,
  "max_file_size": 1073741824
}
```

Two-pane file browser (local ↔ remote) using the file transfer media channel. Supports drag-and-drop, progress tracking, and resumable transfers.

### Widget Groups

Widgets are organized into collapsible groups in the panel UI. Group order follows the order of first appearance in the manifest.

### Widget State Updates

Bridges send widget values in heartbeats (periodic) or via `state_update` messages (event-driven):

```json
{
  "type": "bulk_update",
  "timestamp": "2026-03-19T14:30:00Z",
  "payload": {
    "updates": [
      {"widget_id": "pressure", "value": 4.2},
      {"widget_id": "temperature_history", "value": {"temp_supply": 82.3, "temp_return": 61.7}},
      {"widget_id": "pump_1_status", "value": "green"},
      {"widget_id": "valve_main", "value": true}
    ]
  }
}
```

---

## Command Bus

### Flow

```
┌────────┐    click     ┌────────┐   validate   ┌────────┐  WS command  ┌────────┐  native   ┌────────┐
│ Panel  │ ───────────► │  REST  │ ────────────► │  CDAP  │ ───────────► │ Bridge │ ────────► │ Device │
│  User  │              │  API   │   RBAC +      │Gateway │              │        │  protocol │        │
│        │ ◄─────────── │        │ ◄──────────── │        │ ◄─────────── │        │ ◄──────── │        │
└────────┘  WS push     └────────┘   audit log   └────────┘  response    └────────┘  result   └────────┘
```

### REST API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/api/cdap/devices/{id}/command` | Send command to device |
| `GET` | `/api/cdap/devices/{id}/widgets` | Get current widget states |
| `GET` | `/api/cdap/devices/{id}/manifest` | Get device manifest |
| `GET` | `/api/cdap/devices` | List all CDAP devices |
| `GET` | `/api/cdap/devices?type=scada` | Filter by device type |
| `GET` | `/api/cdap/bridges` | List connected bridges |
| `POST` | `/api/cdap/devices/{id}/widget/{wid}` | Set specific widget value |

### Command Message

```json
{
  "type": "command",
  "id": "cmd-a7f3b210-001",
  "timestamp": "2026-03-19T14:30:15Z",
  "payload": {
    "command_id": "cmd-a7f3b210-001",
    "widget_id": "valve_main",
    "action": "set",
    "value": false,
    "operator": "admin",
    "reason": "Scheduled maintenance"
  }
}
```

### Command Actions

| Action | Widget Types | Description |
|--------|-------------|-------------|
| `set` | toggle, select, slider | Set widget to specific value |
| `trigger` | button | Execute button action |
| `execute` | terminal | Run command in shell |
| `reset` | any | Reset widget to default value |
| `query` | any | Request current value (force refresh) |

### Command Response

```json
{
  "type": "command_response",
  "id": "cmd-a7f3b210-001",
  "timestamp": "2026-03-19T14:30:15.120Z",
  "payload": {
    "command_id": "cmd-a7f3b210-001",
    "status": "ok",
    "execution_time_ms": 120,
    "result": {
      "valve_main": false
    }
  }
}
```

### Command Status Codes

| Status | Description |
|--------|-------------|
| `ok` | Command executed successfully |
| `error` | Command failed (see `error_message`) |
| `timeout` | Device did not respond in time |
| `rejected` | Device rejected the command (safety interlock, etc.) |
| `queued` | Command accepted, will execute asynchronously |
| `unauthorized` | Bridge does not allow this action |

---

## API Bridge Architecture

### The Core Concept

An API Bridge is a small, standalone program that:

1. **Connects upstream** to Yomie server via CDAP WebSocket (`:21122`)
2. **Connects downstream** to the target device via its native protocol
3. **Translates** between the two in real-time

```
                    ┌─────────────────────────────────────┐
                    │            API Bridge                │
                    │                                      │
  Yomie        │  ┌──────────┐     ┌──────────────┐  │        Device
  Server ◄──────────┤  │  CDAP    │◄───►│   Native     │  ├───────► (PLC,
  (:21122)  WS/JSON │  │  Client  │     │   Protocol   │  │ Modbus  sensor,
            ────────►│  │          │     │   Client     │  │◄─────── camera)
                    │  └──────────┘     └──────────────┘  │
                    │                                      │
                    │  ┌──────────────────────────────┐   │
                    │  │      Widget ↔ Register       │   │
                    │  │      Mapping Table           │   │
                    │  └──────────────────────────────┘   │
                    └─────────────────────────────────────┘
```

### Bridge Types

| Bridge Type | Complexity | Description | Example |
|------------|------------|-------------|---------|
| **Simple Telemetry** | ~50 LOC | Read-only sensors → gauges | Temperature sensor via serial |
| **Standard Control** | ~100-200 LOC | Read + write, widgets, commands | PLC via Modbus TCP |
| **Protocol Gateway** | ~300-500 LOC | Multiple devices behind one bridge | OPC-UA server with 50 tags |
| **OS Agent** | ~500-1000 LOC | Deep OS integration, shell, packages | Linux kernel management daemon |
| **Full Integration** | ~1000+ LOC | Complex ecosystem with alerts, logic | Building management system |

### Bridge SDK (Optional)

While bridges can be written from scratch (just JSON over WebSocket), optional SDKs reduce boilerplate:

```
yomie-bridge-sdk/
├── python/          # pip install yomie-bridge
│   └── betterdesk_bridge/
│       ├── __init__.py
│       ├── client.py       # WebSocket client + reconnect
│       ├── manifest.py     # Manifest builder
│       ├── widgets.py      # Widget type helpers
│       └── bridge.py       # Base bridge class
├── nodejs/          # npm install yomie-bridge
├── go/              # go get github.com/yomie/bridge-sdk-go
├── rust/            # yomie-bridge = "0.1"
└── c/               # Header-only library for embedded
```

### Bridge Deployment Models

```
Model A: Bridge on Dedicated Machine          Model B: Bridge on Device Itself
┌───────────┐    LAN    ┌──────────┐          ┌─────────────────────┐
│ Yomie│◄─────────►│ Bridge   │          │       Device        │
│  Server   │   WS/TLS  │ Machine  │          │  ┌───────────────┐  │
└───────────┘           └──────┬───┘          │  │  Bridge       │  │
                               │              │  │  (embedded)   │  │
                          ┌────┴───┐          │  └───────┬───────┘  │
                          │ Device │          │          │          │
                          │ (PLC)  │          │    local access     │
                          └────────┘          └─────────────────────┘
                                                        │
                                              ┌─────────┴─────────┐
                                              │  Yomie Server │
                                              └────────────────────┘

Model C: Bridge as Docker Sidecar             Model D: Cloud-to-Cloud Bridge
┌──────────────────────────┐                  ┌───────────┐    API    ┌──────────┐
│    Docker Host           │                  │ Yomie│◄────────►│  Bridge  │
│  ┌─────────┐ ┌────────┐ │                  │  Server   │          │ (cloud)  │
│  │Yomie│ │ Bridge │ │                  └───────────┘          └────┬─────┘
│  │ Server  │◄┤ Sidecar│ │                                              │
│  └─────────┘ └───┬────┘ │                                         ┌────┴─────┐
│                   │      │                                         │ Vendor   │
│              ┌────┴───┐  │                                         │ Cloud API│
│              │ Device  │  │                                         └──────────┘
│              └─────────┘  │
└──────────────────────────┘
```

### Bridge Lifecycle

```
            ┌─────────┐
   start    │  INIT   │
   ────────►│         │
            └────┬────┘
                 │ connect to Yomie
                 ▼
            ┌─────────┐
            │  AUTH    │──── fail ──► retry with backoff
            │         │
            └────┬────┘
                 │ auth_result: success
                 ▼
            ┌─────────┐
            │REGISTER │──── connect to device
            │         │──── send manifest
            └────┬────┘
                 │ registered: device_id
                 ▼
            ┌─────────┐
   ┌───────►│ RUNNING │◄─── heartbeat loop
   │        │         │◄─── command handling
   │        └────┬────┘──── state updates
   │             │
   │    WS disconnect / device error
   │             ▼
   │        ┌─────────┐
   └────────┤RECONNECT│──── exponential backoff
            │         │──── preserve device_id
            └─────────┘
```

---

## Bridge Examples

### Example 1: Modbus TCP → SCADA PLC (Python, ~80 LOC)

```python
#!/usr/bin/env python3
"""Yomie Bridge: Modbus TCP PLC → CDAP"""

import asyncio
import json
import websockets
from pymodbus.client import AsyncModbusTcpClient

# Configuration
BETTERDESK_URL = "ws://yomie-server:21122"
API_KEY = "your-api-key-here"
PLC_HOST = "192.168.10.50"
PLC_PORT = 502

MANIFEST = {
    "manifest_version": "1.0",
    "device": {
        "name": "Boiler Room PLC",
        "type": "scada",
        "vendor": "Siemens",
        "model": "S7-1200",
        "tags": ["boiler", "hvac"]
    },
    "bridge": {"name": "modbus-bridge", "version": "1.0.0", "protocol": "modbus-tcp"},
    "capabilities": ["telemetry", "commands"],
    "heartbeat_interval": 10,
    "widgets": [
        {"type": "gauge", "id": "pressure", "label": "Boiler Pressure",
         "unit": "bar", "min": 0, "max": 10, "readonly": True},
        {"type": "gauge", "id": "temperature", "label": "Water Temp",
         "unit": "°C", "min": 0, "max": 150, "readonly": True},
        {"type": "toggle", "id": "pump_1", "label": "Pump 1", "readonly": False},
        {"type": "led", "id": "flame", "label": "Burner",
         "states": {"green": "On", "red": "Fault", "gray": "Off"}}
    ]
}

# Modbus register → widget mapping
REGISTER_MAP = {
    "pressure":    {"address": 100, "type": "holding", "scale": 0.1},
    "temperature": {"address": 101, "type": "holding", "scale": 0.1},
    "pump_1":      {"address": 200, "type": "coil"},
    "flame":       {"address": 201, "type": "discrete",
                    "map": {0: "gray", 1: "green", 2: "red"}}
}

async def main():
    modbus = AsyncModbusTcpClient(PLC_HOST, port=PLC_PORT)
    await modbus.connect()

    async with websockets.connect(BETTERDESK_URL) as ws:
        # Authenticate
        await ws.send(json.dumps({"type": "auth", "payload": {"api_key": API_KEY}}))
        auth_resp = json.loads(await ws.recv())
        assert auth_resp["payload"]["success"], "Auth failed"

        # Register device
        await ws.send(json.dumps({"type": "register", "payload": {"manifest": MANIFEST}}))
        reg_resp = json.loads(await ws.recv())
        device_id = reg_resp["payload"]["device_id"]
        print(f"Registered as {device_id}")

        # Main loop: read sensors + handle commands
        async def heartbeat_loop():
            while True:
                holding = await modbus.read_holding_registers(100, 2)
                coils = await modbus.read_coils(200, 1)
                discrete = await modbus.read_discrete_inputs(201, 1)

                updates = [
                    {"widget_id": "pressure", "value": round(holding.registers[0] * 0.1, 1)},
                    {"widget_id": "temperature", "value": round(holding.registers[1] * 0.1, 1)},
                    {"widget_id": "pump_1", "value": bool(coils.bits[0])},
                    {"widget_id": "flame", "value": {0: "gray", 1: "green", 2: "red"}.get(
                        int(discrete.bits[0]), "gray")}
                ]
                await ws.send(json.dumps({"type": "bulk_update", "payload": {"updates": updates}}))
                await asyncio.sleep(10)

        async def command_handler():
            async for message in ws:
                msg = json.loads(message)
                if msg["type"] == "command":
                    cmd = msg["payload"]
                    if cmd["widget_id"] == "pump_1" and cmd["action"] == "set":
                        await modbus.write_coil(200, cmd["value"])
                        await ws.send(json.dumps({
                            "type": "command_response",
                            "payload": {"command_id": cmd["command_id"], "status": "ok",
                                        "result": {"pump_1": cmd["value"]}}
                        }))

        await asyncio.gather(heartbeat_loop(), command_handler())

if __name__ == "__main__":
    asyncio.run(main())
```

### Example 2: Linux OS Agent (Go, ~120 LOC concept)

```go
// yomie-os-agent: kernel-level system management bridge
package main

// Manifest excerpt for OS Agent
var manifest = Manifest{
    Device: Device{
        Name: hostname,
        Type: "os_agent",
        Tags: []string{"linux", "server", "production"},
    },
    Widgets: []Widget{
        {Type: "gauge", ID: "cpu_usage", Label: "CPU Usage", Unit: "%", Min: 0, Max: 100, Readonly: true},
        {Type: "gauge", ID: "memory_usage", Label: "Memory", Unit: "%", Min: 0, Max: 100, Readonly: true},
        {Type: "gauge", ID: "disk_usage", Label: "Disk /", Unit: "%", Min: 0, Max: 100, Readonly: true},
        {Type: "chart", ID: "network_io", Label: "Network I/O", Series: []Series{
            {ID: "rx", Label: "RX", Color: "#3b82f6"},
            {ID: "tx", Label: "TX", Color: "#ef4444"},
        }},
        {Type: "table", ID: "services", Label: "Systemd Services", Columns: []Column{
            {ID: "name", Label: "Service"},
            {ID: "status", Label: "Status"},
            {ID: "memory", Label: "Memory"},
        }},
        {Type: "terminal", ID: "shell", Label: "Remote Shell",
            AllowedCommands: []string{"systemctl", "journalctl", "ip", "ss", "df", "free", "top"}},
        {Type: "button", ID: "reboot", Label: "Reboot", Style: "danger", Confirm: true},
        {Type: "select", ID: "kernel_sched", Label: "CPU Scheduler", Options: []Option{
            {Value: "cfs", Label: "CFS (default)"},
            {Value: "deadline", Label: "Deadline"},
            {Value: "rt", Label: "Real-Time"},
        }},
    },
}

// Telemetry reads from /proc, /sys — direct kernel interface
// Commands execute via systemd D-Bus API or direct syscalls
// Shell widget uses PTY with command allowlist
```

### Example 3: REST API Bridge (Node.js, ~60 LOC concept)

```javascript
// Bridge: IP Camera REST API → Yomie CDAP
const WebSocket = require('ws');
const axios = require('axios');

const CAMERA_API = 'http://192.168.1.100/api';
const manifest = {
  manifest_version: '1.0',
  device: { name: 'Lobby Camera', type: 'camera', vendor: 'Hikvision', model: 'DS-2CD2143G2-I' },
  widgets: [
    { type: 'led', id: 'recording', label: 'Recording', states: { green: 'Active', red: 'Stopped' } },
    { type: 'toggle', id: 'ir_mode', label: 'IR Night Vision' },
    { type: 'select', id: 'resolution', label: 'Resolution',
      options: [ { value: '4mp', label: '4MP' }, { value: '1080p', label: '1080p' }, { value: '720p', label: '720p' } ] },
    { type: 'button', id: 'snapshot', label: 'Take Snapshot', style: 'primary' },
    { type: 'text', id: 'last_motion', label: 'Last Motion Detected' }
  ]
};

// Bridge translates:
//   GET camera/api/status     → gauge/led widget values
//   PUT camera/api/ir-mode    ← toggle command from panel
//   PUT camera/api/resolution ← select command from panel
//   POST camera/api/snapshot  ← button trigger from panel
```

### Example 4: ESP32 Microcontroller (Arduino C, ~90 LOC concept)

```c
// Bridge running directly on ESP32 — no separate bridge machine needed
// Uses ArduinoWebSockets library + ArduinoJson

const char* manifest = R"({
  "manifest_version": "1.0",
  "device": {
    "name": "Greenhouse Sensor Node #3",
    "type": "iot",
    "vendor": "Custom",
    "model": "ESP32-WROOM"
  },
  "widgets": [
    {"type": "gauge", "id": "soil_moisture", "label": "Soil Moisture", "unit": "%",
     "min": 0, "max": 100, "readonly": true},
    {"type": "gauge", "id": "air_temp", "label": "Air Temperature", "unit": "°C",
     "min": -10, "max": 60, "readonly": true},
    {"type": "gauge", "id": "humidity", "label": "Humidity", "unit": "%",
     "min": 0, "max": 100, "readonly": true},
    {"type": "toggle", "id": "irrigation", "label": "Irrigation Valve", "readonly": false},
    {"type": "led", "id": "battery", "label": "Battery",
     "states": {"green": ">50%", "yellow": "20-50%", "red": "<20%"}}
  ]
})";

// ESP32 reads sensors via ADC/I2C → sends as heartbeat
// Toggle command → GPIO pin → solenoid valve
// ~4KB RAM footprint for CDAP client
```

### Example 5: Yomie Native Desktop Client (Rust/Go OS Agent, ~500 LOC)

The Yomie native client is a **desktop bridge** — an OS agent that exposes the local machine as a CDAP device with full remote desktop capability PLUS system management widgets:

```rust
// Conceptual Rust-based Yomie desktop agent
// Dual-purpose: remote desktop + system management

struct BetterDeskAgent {
    cdap: CdapClient,             // CDAP WebSocket connection
    screen_capture: DxgiCapture,  // Platform screen capture
    encoder: H264Encoder,         // Hardware-accelerated encoder
    input_sink: InputInjector,    // Keyboard/mouse injection
    audio_capture: AudioCapture,  // System audio capture
}

async fn connect() {
    let manifest = json!({
        "manifest_version": "1.0",
        "device": {
            "name": hostname(),
            "type": "desktop",
            "vendor": "Yomie",
            "model": os_info(),
            "firmware": env!("CARGO_PKG_VERSION")
        },
        "capabilities": [
            "telemetry", "commands",
            "remote_desktop", "video_stream", "audio",
            "clipboard", "file_transfer", "input_control"
        ],
        "media": {
            "video": {
                "codecs": ["h264", "vp9"],
                "preferred": "h264",
                "hardware_encoder": true,
                "displays": enumerate_displays()
            },
            "audio": { "codecs": ["opus"], "bidirectional": true },
            "input": { "keyboard": true, "mouse": true },
            "clipboard": { "text": true, "image": true },
            "file_transfer": { "max_file_size": 4294967296, "resumable": true }
        },
        "widgets": [
            {"type": "desktop", "id": "screen", "label": "Remote Desktop",
             "group": "Remote Access"},
            {"type": "file_browser", "id": "files", "label": "File Manager",
             "group": "Remote Access"},
            {"type": "terminal", "id": "shell", "label": "Remote Shell",
             "group": "Management", "shell": detect_shell()},
            {"type": "gauge", "id": "cpu", "label": "CPU Usage",
             "group": "System", "unit": "%", "min": 0, "max": 100},
            {"type": "gauge", "id": "ram", "label": "RAM Usage",
             "group": "System", "unit": "%", "min": 0, "max": 100},
            {"type": "gauge", "id": "disk", "label": "Disk Usage",
             "group": "System", "unit": "%", "min": 0, "max": 100},
            {"type": "table", "id": "services", "label": "Services",
             "group": "Management",
             "columns": [
                 {"id": "name", "label": "Service"},
                 {"id": "status", "label": "Status"},
                 {"id": "cpu", "label": "CPU %"}
             ]},
            {"type": "text", "id": "os_info", "label": "OS Info",
             "group": "System"}
        ]
    });

    // Registration → control channel for widgets
    // Media connect → binary channel for screen/audio/input
    // Both on the same WebSocket connection
    cdap.register(manifest).await;
}

// When viewer clicks "Remote Desktop" widget:
async fn on_media_session(session: MediaSession) {
    // Start screen capture → encode → send binary frames
    // Receive input events → inject into OS
    // Bidirectional clipboard + audio + file transfer
    loop {
        let frame = screen_capture.grab_frame();
        let encoded = encoder.encode(frame);
        session.send_video(encoded).await;

        if let Some(input) = session.recv_input().await {
            input_sink.inject(input);
        }
    }
}
```

**This is the key differentiator**: A Yomie native client is NOT just a remote desktop tool — it is a full management agent. From the panel, an operator can remote-control the screen, browse files, open a terminal, view CPU/RAM metrics, and manage system services — all from one device detail page, through one protocol, one port, one auth.

---

## Security Model

### Authentication Layers

```
┌──────────────────────────────────────────────────┐
│              Security Stack                       │
│                                                   │
│  Layer 1: Transport Security                      │
│  ├── TLS 1.3 (optional, auto-detected)           │
│  └── DualModeListener (plain + TLS on same port) │
│                                                   │
│  Layer 2: Client Authentication                   │
│  ├── User/password (same as Node.js panel login)  │
│  ├── API key (shared secret, per-bridge)          │
│  ├── Device enrollment token (one-time)           │
│  ├── mTLS client certificates (optional)          │
│  └── IP allowlist (optional)                      │
│                                                   │
│  Layer 3: Two-Factor Authentication (TOTP)        │
│  ├── Mandatory for user/password auth if enabled  │
│  ├── Same TOTP secret as panel + RustDesk login   │
│  ├── 5-minute partial token during 2FA flow       │
│  ├── Recovery code support (8 one-time codes)     │
│  └── 2FA setup via panel only (not via CDAP)      │
│                                                   │
│  Layer 4: Session Management                      │
│  ├── JWT session token (24h, HS256)               │
│  ├── Token refresh (30-day chain max)             │
│  ├── Session bound to bridge IP                   │
│  └── Admin revocable from panel                   │
│                                                   │
│  Layer 5: Command Authorization (RBAC)            │
│  ├── Per-widget permissions (read/write)           │
│  ├── Per-device-type role restrictions             │
│  ├── Role hierarchy: Admin > Operator > Viewer    │
│  ├── Operator can read gauges but not set valves  │
│  └── Admin has full access                        │
│                                                   │
│  Layer 6: Command Validation                      │
│  ├── Value range checks (slider min/max)          │
│  ├── Confirmation requirement (confirm: true)     │
│  ├── Cooldown enforcement (cooldown: 5s)          │
│  └── Rate limiting (10 commands/min/device)       │
│                                                   │
│  Layer 7: Audit Trail                             │
│  ├── Every command logged with operator identity   │
│  ├── Every state change logged with timestamp     │
│  ├── Failed commands + auth failures logged       │
│  └── Bridge connect/disconnect events             │
│                                                   │
│  Layer 8: Device-Side Safety                      │
│  ├── Bridge validates commands before execution    │
│  ├── command_response: "rejected" for unsafe ops  │
│  ├── Blocked command list (terminal widget)       │
│  └── Physical safety interlocks are NEVER bypassed│
│                                                   │
└──────────────────────────────────────────────────┘
```

### Authentication Methods

CDAP supports three authentication methods, all validated inside the WebSocket connection:

#### Method 1: User/Password + 2FA (Interactive Clients)

For desktop agents and operator-attended bridges. Uses the **same credentials as Node.js panel login and RustDesk client login** — one account across all protocols.

```json
// Step 1: Client sends credentials
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

// Step 2a: No 2FA → immediate success
{
  "type": "auth_result",
  "payload": {
    "success": true,
    "token": "jwt_24h_token",
    "role": "operator",
    "device_id": "CDAP-A7F3B210"
  }
}

// Step 2b: 2FA enabled → partial token
{
  "type": "auth_result",
  "payload": {
    "success": false,
    "requires_2fa": true,
    "tfa_type": "totp",
    "partial_token": "jwt_5min_ttl"
  }
}

// Step 3: Client sends TOTP code
{
  "type": "auth_2fa",
  "payload": {
    "partial_token": "jwt_5min_ttl",
    "code": "123456"
  }
}

// Step 4: Full auth granted
{
  "type": "auth_result",
  "payload": {
    "success": true,
    "token": "jwt_24h_token",
    "role": "operator",
    "device_id": "CDAP-A7F3B210"
  }
}
```

The Go server reuses the existing `auth.VerifyPassword()` (PBKDF2-HMAC-SHA256, constant-time) and `auth.ValidateTOTP()` (RFC 6238, ±1 time step window) functions — **no auth code duplication**.

#### Method 2: API Key (Unattended Bridges)

For headless IoT/SCADA bridges that run 24/7 without operator interaction:

```json
{
  "type": "auth",
  "payload": {
    "method": "api_key",
    "key": "bdsk_a1b2c3d4e5f6...",
    "device_id": "CDAP-A7F3B210"
  }
}
```

API keys are created in the Node.js panel (Settings → API Keys) and stored in Go server's `api_keys` table. No 2FA prompt for API key auth.

#### Method 3: Device Enrollment Token (First-Time Setup)

One-time enrollment tokens for new devices. Created by admin in panel, expires after use:

```json
{
  "type": "auth",
  "payload": {
    "method": "device_token",
    "token": "enroll_xyz789...",
    "device_id": "CDAP-A7F3B210"
  }
}
```

After enrollment, the server issues a persistent API key for the device, stored locally. Subsequent connections use API key auth.

### 2FA Integration with Existing System

CDAP 2FA uses the **exact same TOTP infrastructure** as the Node.js panel and RustDesk client:

```
┌──────────────────────────────────────────────────────┐
│           Unified 2FA Architecture                    │
│                                                       │
│  TOTP Secret: users.totp_secret (Go server DB)       │
│  Algorithm:   HMAC-SHA1, 6 digits, 30s period, ±1   │
│  Recovery:    8 one-time codes (Go: unused, Node: DB)│
│                                                       │
│  Setup:  Panel only (POST /api/auth/totp/setup)      │
│  Verify: All three protocols share verification:     │
│                                                       │
│  ┌─────────┐  ┌─────────────┐  ┌──────────────┐    │
│  │  Panel   │  │ RustDesk    │  │ CDAP Client  │    │
│  │ (Node.js)│  │ /api/login  │  │ WS auth_2fa  │    │
│  └────┬─────┘  └──────┬──────┘  └──────┬───────┘    │
│       │               │                │             │
│       ▼               ▼                ▼             │
│  ┌────────────────────────────────────────────┐      │
│  │     auth.ValidateTOTP(secret, code)        │      │
│  │     (Go server — single implementation)    │      │
│  └────────────────────────────────────────────┘      │
│                                                       │
│  Rate limiting: 5 attempts / 5 min / IP (all paths)  │
│  Partial token TTL: 5 minutes (prevents brute-force) │
│  Audit: every 2FA attempt logged (success + failure) │
└──────────────────────────────────────────────────────┘
```

**Key principle**: A user enables 2FA once in the panel → it immediately applies to panel login, RustDesk client login, AND CDAP client login. No separate 2FA setup per protocol.

### Token Lifecycle for Long-Running Agents

Desktop agents and IoT bridges run 24/7. JWT tokens expire after 24h. Token refresh prevents forced re-login:

```json
// Client sends before token expires
{
  "type": "token_refresh",
  "payload": { "token": "current_jwt" }
}

// Server responds (no re-auth needed)
{
  "type": "token_refreshed",
  "payload": {
    "token": "new_jwt_24h",
    "expires_at": "2026-03-21T14:30:00Z"
  }
}
```

- Max token refresh chain: **30 days** (then full re-auth with password + 2FA)
- Admin can revoke sessions from panel → forces re-login on next refresh attempt
- API key auth bypasses token refresh entirely (keys have optional expiry date)

### RustDesk Client Synchronization

CDAP and RustDesk clients sharing the same server must appear unified in the panel:

```
┌──────────────────────────────────────────────────────────┐
│           Unified Device & Identity Model                 │
│                                                           │
│  ┌─────────────────┐     ┌─────────────────────────┐    │
│  │ RustDesk Client  │     │ Yomie Native Client│    │
│  │ ID: 892734561    │     │ ID: CDAP-D2E9F4         │    │
│  │ Protocol: Signal │     │ Protocol: CDAP          │    │
│  │ Port: 21116      │     │ Port: 21122             │    │
│  └────────┬─────────┘     └───────────┬─────────────┘    │
│           │                           │                   │
│           ▼                           ▼                   │
│  ┌────────────────────────────────────────────────┐      │
│  │              peers table (shared)               │      │
│  │                                                 │      │
│  │  id           │ device_type │ linked_peer_id   │      │
│  │  892734561    │ rustdesk    │ CDAP-D2E9F4      │      │
│  │  CDAP-D2E9F4  │ desktop     │ 892734561        │      │
│  │  CDAP-A7F3B2  │ scada       │ (null)           │      │
│  │  CDAP-F9A2B1  │ os_agent    │ (null)           │      │
│  └────────────────────────────────────────────────┘      │
│                                                           │
│  Shared resources per user:                              │
│  ├── Address book   (GET /api/ab — same for both)        │
│  ├── Connection log (same audit table)                   │
│  ├── Credentials    (same users table)                   │
│  ├── 2FA state      (same TOTP secret)                   │
│  ├── Tags/Groups    (same peers.tags field)              │
│  └── RBAC role      (same role across all protocols)     │
│                                                           │
│  Panel shows:                                            │
│  ├── Linked devices as single machine with 2 protocols   │
│  ├── Unified online status (either protocol = online)    │
│  └── Device type filter (rustdesk / desktop / scada ...) │
└──────────────────────────────────────────────────────────┘
```

**Auto-linking**: When both RustDesk and CDAP clients on the same machine are logged in with the same user, and hostnames match, the server automatically links them in `linked_peer_id`. Admin can also manually link/unlink from the panel.

**Address books**: Both client types use the same `/api/ab` endpoint with the same JWT token. Adding a device in RustDesk address book is visible to CDAP client and vice versa.

### Critical Safety Principle

> **CDAP is a control plane, NOT a safety system.** Physical safety interlocks (emergency stops, pressure relief valves, overcurrent protection) must ALWAYS be implemented in hardware or local PLC logic, NEVER rely on network commands from Yomie. CDAP commands are "requests" — the device/bridge has the authority to reject any command that violates safety constraints.

### RBAC for Widgets

```json
{
  "widget_permissions": {
    "admin": {"read": "*", "write": "*"},
    "operator": {
      "read": "*",
      "write": ["pump_1", "operating_mode", "setpoint_temp"],
      "deny_write": ["emergency_stop"]
    },
    "viewer": {"read": "*", "write": []}
  }
}
```

---

## Device Revocation Protocol

When an admin deletes a device from the panel, the device should not only be blocked from re-registering — it should be **actively notified** to disconnect and clear its server configuration. This prevents "zombie" devices from polling forever and ensures complete removal from the ecosystem.

### Problem: Current Gaps

The existing soft-delete mechanism blocks re-registration at the signal handler level (`IsPeerSoftDeleted` check) but has critical gaps:

| Gap | Impact |
|-----|--------|
| No disconnect message sent | Device remains connected until next heartbeat timeout (~15s) |
| TCP/WS connections not closed | Existing connections persist after deletion at OS socket level |
| No config wipe command | Deleted device still has server address configured, retries indefinitely |
| No revocation propagation | Relay sessions initiated before deletion can continue |
| No linked device cascade | Deleting RustDesk peer doesn't revoke linked CDAP device |

### Revocation Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Device Revocation Flow                               │
│                                                                         │
│  Admin clicks "Delete Device" in panel                                  │
│       │                                                                 │
│       ▼                                                                 │
│  ┌─────────────────┐                                                    │
│  │  Node.js Panel   │ DELETE /api/devices/:id?revoke=true               │
│  └────────┬─────────┘                                                    │
│           │                                                             │
│           ▼                                                             │
│  ┌─────────────────┐                                                    │
│  │  Go Server API   │ DELETE /api/peers/:id?revoke=true                 │
│  └────────┬─────────┘                                                    │
│           │                                                             │
│           ├─── 1. Soft-delete in DB (soft_deleted=true)                 │
│           ├─── 2. Add to blocklist (ID-based, permanent)                │
│           ├─── 3. Revoke all active sessions (JWT blacklist)            │
│           ├─── 4. Close TCP/WS connections (if peer in memory)          │
│           │                                                             │
│           ├─── 5a. CDAP device? Send revocation message via WS          │
│           │         ┌──────────────────────────┐                        │
│           │         │  {                        │                        │
│           │         │    "type": "revoke",      │                        │
│           │         │    "payload": {           │                        │
│           │         │      "reason": "deleted", │                        │
│           │         │      "wipe_config": true, │                        │
│           │         │      "message": "..."     │                        │
│           │         │    }                      │                        │
│           │         │  }                        │                        │
│           │         └──────────────────────────┘                        │
│           │                                                             │
│           ├─── 5b. RustDesk device? Close signal connection +           │
│           │        send RegisterPkResponse{NOT_SUPPORT} on retry        │
│           │                                                             │
│           ├─── 6. Cascade: revoke linked_peer_id device too             │
│           │                                                             │
│           └─── 7. Publish EventPeerRevoked to event bus                 │
│                                                                         │
│  Device receives revocation:                                           │
│  ├── CDAP client: clear server config → show "Revoked" UI → exit       │
│  ├── RustDesk client: connection rejected → stays configured (*)       │
│  └── IoT bridge: disconnect → optional auto-wipe → log event           │
│                                                                         │
│  (*) RustDesk client cannot be remotely wiped — protocol limitation.    │
│      But re-registration is permanently blocked by blocklist + soft     │
│      delete check. Device shows as "offline" forever and never returns. │
└─────────────────────────────────────────────────────────────────────────┘
```

### CDAP Revocation Messages

#### Server → Device: `revoke`

Sent by the server to a connected CDAP device when it is deleted:

```json
{
  "type": "revoke",
  "payload": {
    "reason": "deleted",
    "wipe_config": true,
    "message": "Device removed by administrator.",
    "timestamp": "2026-03-19T14:30:00Z"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `reason` | string | `"deleted"` \| `"banned"` \| `"security"` \| `"expired"` |
| `wipe_config` | bool | If `true`, client MUST erase server address, keys, and cached credentials from local storage |
| `message` | string | Human-readable message (shown to user if desktop client) |
| `timestamp` | string | ISO 8601 server timestamp |

**Client behavior after receiving `revoke`:**

1. **Desktop agent (Yomie native)**:
   - Display notification: "This device has been removed from the server."
   - Clear stored: server address, API key, device token, JWT, cached keypair
   - Close WebSocket connection gracefully
   - Revert to "unconfigured" state (setup wizard on next launch)
   - Write revocation event to local log

2. **IoT/SCADA bridge**:
   - Close WebSocket connection
   - If `wipe_config: true`: remove server config file, clear credentials
   - Enter standby mode (no reconnect attempts)
   - Log revocation reason + timestamp

3. **OS agent (daemon)**:
   - Close WebSocket connection
   - If `wipe_config: true`: clear `/etc/yomie/config.json` or equivalent
   - Stop service gracefully (no auto-restart)
   - Can be re-enrolled with new device token if needed

#### Server → Device: `suspend`

Temporary suspension (ban) — device disconnects but keeps config:

```json
{
  "type": "suspend",
  "payload": {
    "reason": "banned",
    "message": "Device suspended due to policy violation.",
    "retry_after": null,
    "timestamp": "2026-03-19T14:30:00Z"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `reason` | string | `"banned"` \| `"maintenance"` \| `"policy"` |
| `retry_after` | int\|null | Seconds before client may retry, or `null` for indefinite |
| `message` | string | Human-readable explanation |

**Client behavior:**
- Disconnect immediately
- Keep server config (not wiped)
- If `retry_after` is set: schedule reconnect after delay
- If `retry_after` is `null`: enter "suspended" state, display reason to user
- Unbanning from panel triggers no message — client reconnects on its own timer or manual retry

#### Device → Server: `revoke_ack`

Optional confirmation that revocation was processed:

```json
{
  "type": "revoke_ack",
  "payload": {
    "config_wiped": true,
    "device_id": "CDAP-A7F3B210"
  }
}
```

The server does NOT wait for this ACK — the device is already blocked. This is best-effort for audit trail purposes.

### RustDesk Device Revocation

RustDesk's signal protocol does not have a revocation message. When a RustDesk device is deleted:

1. **Immediate**: `peer.Map.Remove(id)` clears routing → device becomes unreachable for incoming connections
2. **Connection close**: If `peer.Entry.TCPConn` is present, call `TCPConn.Close()` to force TCP RST
3. **Re-registration block**: `IsPeerSoftDeleted()` and `IsPeerBanned()` checks both reject `RegisterPeer` and `RegisterPk` — device never re-appears in peer list
4. **ID blocklist**: Add device ID to `security.Blocklist` for belt-and-suspenders protection
5. **Client-side**: RustDesk client sees connection failure, retries with exponential backoff, eventually shows "offline". Server address remains configured but the device ID is permanently blocked.

**Limitation**: RustDesk client **cannot** be remotely config-wiped. This is a protocol limitation of the existing RustDesk signal protocol. The device will retry forever (with backoff) but never successfully register. To fully remove, the end-user must manually reconfigure the client or uninstall it.

> **Yomie native client advantage**: Unlike RustDesk, CDAP's `revoke` message with `wipe_config: true` enables full remote wipe — the device clears its configuration and stops all reconnect attempts. This is a key security improvement for enterprise environments.

### Panel Revocation UI

```
┌─────────────────────────────────────────────────────────────────┐
│  Delete Device: CDAP-A7F3B210 (Boiler Room PLC)                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ⚠️  This action will permanently remove the device from        │
│     the server and block it from reconnecting.                  │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ ☑ Revoke and wipe device configuration                   │    │
│  │   Device will clear its server settings and disconnect.  │    │
│  │   (Recommended for decommissioned devices)               │    │
│  │                                                          │    │
│  │ ☑ Add device ID to permanent blocklist                   │    │
│  │   Prevents re-registration even if client is             │    │
│  │   reinstalled with the same device ID.                   │    │
│  │                                                          │    │
│  │ ☑ Cascade to linked devices                              │    │
│  │   Also revoke linked RustDesk peer: 892734561            │    │
│  │   (PC-Warehouse, linked via hostname match)              │    │
│  │                                                          │    │
│  │ ☐ Hard delete (permanent, cannot be undone)              │    │
│  │   Remove all traces from database. Default is soft       │    │
│  │   delete (recoverable within 30 days).                   │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  Message to device (optional):                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ Device removed by administrator.                         │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│                        [Cancel]   [Delete & Revoke]              │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Cascade Delete with Linked Devices

When a device has a `linked_peer_id`, revocation can cascade:

```
Admin deletes CDAP-D2E9F4 (desktop, linked to 892734561)
  │
  ├── 1. Revoke CDAP-D2E9F4 → send "revoke" WS message
  │       └── Yomie client clears config, disconnects
  │
  ├── 2. Cascade: revoke 892734561 (RustDesk)
  │       ├── Remove from peer map
  │       ├── Close TCP connection
  │       ├── Soft-delete in DB
  │       └── Add ID to blocklist
  │
  └── 3. Both entries marked soft_deleted + blocklisted
         Panel shows: "2 devices revoked (cascade)"
```

**Cascade is opt-in** (checkbox in delete dialog). By default, only the selected device is revoked. Admin explicitly confirms cascade to avoid accidental removal of linked devices.

**Reverse scenario**: Deleting RustDesk peer 892734561 can also cascade to CDAP-D2E9F4 if the cascade option is selected.

### Revocation vs. Ban vs. Soft Delete — Comparison

| Feature | Soft Delete | Ban | Revoke (CDAP) |
|---------|-------------|-----|---------------|
| Re-registration blocked | ✅ | ✅ | ✅ |
| Active disconnect message | ❌ | ❌ | ✅ |
| Client config wiped | ❌ | ❌ | ✅ (wipe_config) |
| Recoverable (admin undelete) | ✅ (30 days) | ✅ (unban) | Depends on hard/soft |
| Relay sessions terminated | ❌ | ❌ | ✅ (connection closed) |
| ID added to blocklist | ❌ | ❌ | ✅ (optional) |
| Linked device cascade | ❌ | ❌ | ✅ (opt-in) |
| Works for RustDesk clients | ✅ (passive) | ✅ (passive) | ⚠️ (no config wipe) |
| Works for CDAP clients | ✅ (passive) | ✅ (passive) | ✅ (full) |
| Audit trail | Basic | Basic | Full (revoke_ack) |

---

## Panel Integration

### Device List View

```
┌─────────────────────────────────────────────────────────────┐
│  Devices                                    [Filter ▼] [+]  │
├─────────────────────────────────────────────────────────────┤
│  🖥️  1340238749  |  PC-Reception   |  Online   | rustdesk  │
│  🖥️  892734561   |  PC-Warehouse   |  Offline  | rustdesk  │
│  🏭  CDAP-A7F3B2 |  Boiler Room PLC|  Online   | scada     │
│  🔌  CDAP-C1D4E8 |  Greenhouse #3  |  Online   | iot       │
│  💻  CDAP-F9A2B1 |  srv-prod-01    |  Online   | os_agent  │
│  📹  CDAP-E3D7C6 |  Lobby Camera   |  Degraded | camera    │
│  🖥️  CDAP-D2E9F4 |  PC-Design-03   |  Online   | desktop   │
├─────────────────────────────────────────────────────────────┤
│  Total: 7  |  Online: 5  |  Offline: 1  |  Degraded: 1     │
└─────────────────────────────────────────────────────────────┘
```

### Device Detail — Widget Panel (CDAP Device)

When clicking a CDAP device, the detail panel shows dynamically-rendered widgets instead of the standard RustDesk device detail:

```
┌─────────────────────────────────────────────────────────┐
│  CDAP-A7F3B210 — Boiler Room PLC                        │
│  Type: SCADA  |  Bridge: modbus-bridge v1.0.0           │
│  Status: Online  |  Last heartbeat: 3s ago              │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  ▼ Sensors                                              │
│  ┌──────────────────┐  ┌──────────────────┐             │
│  │ Boiler Pressure  │  │ Water Temp       │             │
│  │     ◉ 3.7 bar    │  │     ◉ 82.3 °C   │             │
│  │  [░░░░▓▓░░░░░░]  │  │  [░░░░░░▓▓░░░░] │             │
│  │  0           10   │  │  0          150  │             │
│  └──────────────────┘  └──────────────────┘             │
│                                                          │
│  ▼ Control                                              │
│  ┌──────────────────────────────────────────┐           │
│  │ Pump 1          [■ ON ]                  │           │
│  │ Operating Mode  [Automatic        ▼]     │           │
│  │ Temp Setpoint   [====●=========] 75°C    │           │
│  └──────────────────────────────────────────┘           │
│                                                          │
│  ▼ Status                                               │
│  │ 🟢 Pump 1: Running                                  │
│  │ 🟢 Burner: On                                       │
│                                                          │
│  ▼ Monitoring                                           │
│  │ ┌─── Temperature Trend (24h) ──────────────┐        │
│  │ │  90┤   ╱╲   ╱╲   ╱╲   ╱╲   ╱╲          │        │
│  │ │  80┤──╱──╲─╱──╲─╱──╲─╱──╲─╱──╲───      │        │
│  │ │  70┤─╱────╲────╲────╲────╲────╲──       │        │
│  │ │  60┤╱──────────────────────────────       │        │
│  │ │    └──────────────────────────────────┘   │        │
│  │ │    06:00  09:00  12:00  15:00  18:00      │        │
│  │ │    ── Supply  ── Return                   │        │
│  │ └──────────────────────────────────────────┘        │
│                                                          │
│  ▼ Safety                                               │
│  │ [🛑 Emergency Stop]  (requires confirmation)         │
│                                                          │
│  ▼ Diagnostics                                          │
│  │ Last Error: E104: Flame sensor timeout at 14:23:07   │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

### Dashboard Integration

The main dashboard includes CDAP device counts:

```
┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
│  RustDesk     │ │  Native      │ │  SCADA/PLC   │ │  IoT Devices │ │  OS Agents   │
│  Desktops     │ │  Desktops    │ │              │ │              │ │              │
│     47        │ │     12       │ │     12       │ │     89       │ │     15       │
│  🟢 32 online │ │  🟢 11 online│ │  🟢 12 online│ │  🟢 71 online│ │  🟢 14 online│
└──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘ └──────────────┘
```

---

## Technology Stack

| Component | Technology | License | Purpose |
|-----------|-----------|---------|---------|
| CDAP Gateway | Go `gorilla/websocket` | BSD-2 | WebSocket server for bridge connections |
| Widget State Store | SQLite/PostgreSQL | Public Domain/PostgreSQL | Widget values, history, manifests |
| Event Push | Go event bus (existing) | Internal | Real-time widget updates to panel |
| Widget Renderer | Vanilla JS + EJS | Internal | Dynamic UI from manifest JSON |
| Chart Library | Chart.js (existing in panel) | MIT | Time-series graphs for chart widgets |
| Video Decoder (browser) | WebCodecs API / JMuxer | W3C / MIT | Hardware-accelerated video decoding |
| Video Encoder (agent) | Platform DXGI/VA-API + libx264/libvpx | Various | Screen capture + encoding |
| Audio Codec | Opus (`libopus`) | BSD | Bidirectional audio encoding/decoding |
| E2E Crypto | XSalsa20-Poly1305 (NaCl) | Public Domain | End-to-end media encryption |
| Key Exchange | X25519 (Curve25519) | Public Domain | Ephemeral keys for media sessions |
| Canvas Renderer | HTML5 Canvas + OffscreenCanvas | W3C | Remote desktop video display |
| Bridge SDK (Python) | `websockets` + `dataclasses` | BSD | Optional bridge development kit |
| Bridge SDK (Node.js) | `ws` | MIT | Optional bridge development kit |
| Bridge SDK (Go) | `gorilla/websocket` | BSD-2 | Optional bridge development kit |

---

## Implementation Phases

### Phase 1: Foundation (MVP)

**Goal**: Single CDAP device connects, registers, and shows widgets in panel.

| Task | Effort | Priority |
|------|--------|----------|
| CDAP Gateway WebSocket server (`:21122`) | 2-3 days | P0 |
| Auth handler (API key validation) | 0.5 day | P0 |
| Manifest parser + validation | 1 day | P0 |
| `device_type` column in peers table | 0.5 day | P0 |
| Widget state storage (in-memory + DB) | 1-2 days | P0 |
| Heartbeat handler + widget value updates | 1 day | P0 |
| Device list filter by type (panel) | 0.5 day | P0 |
| Dynamic widget renderer (panel, 4 basic types) | 2-3 days | P0 |
| Basic command routing (set, trigger) | 1 day | P0 |
| Python bridge SDK (minimal) | 1 day | P1 |

**Widget types in MVP**: `toggle`, `gauge`, `button`, `led`

**Deliverables**: Working Modbus bridge demo with 4 widgets visible in panel.

### Phase 2: Production Hardening

**Goal**: Secure, reliable, multi-device CDAP deployment.

| Task | Effort | Priority |
|------|--------|----------|
| TLS support on CDAP port | 0.5 day | P0 |
| RBAC per-widget permissions | 1-2 days | P0 |
| Command audit logging | 0.5 day | P0 |
| Rate limiting on commands | 0.5 day | P0 |
| Widget types: `chart`, `select`, `slider`, `text` | 2-3 days | P0 |
| Reconnect handling (preserve device ID) | 1 day | P0 |
| Bridge health monitoring | 0.5 day | P1 |
| Manifest versioning + backward compatibility | 1 day | P1 |
| Node.js bridge SDK | 1 day | P1 |
| i18n for widget labels (panel-side translation) | 0.5 day | P1 |

**Deliverables**: Production-grade CDAP with 8 widget types, security, audit trail.

### Phase 3: Advanced Features

**Goal**: Full ecosystem management capabilities.

| Task | Effort | Priority |
|------|--------|----------|
| Widget type: `table` (dynamic rows) | 1-2 days | P1 |
| Widget type: `terminal` (remote shell) | 2-3 days | P1 |
| Alert system (threshold-based from manifest) | 1-2 days | P1 |
| Alert notifications (WS push + panel bell icon) | 1 day | P1 |
| Go bridge SDK | 1 day | P2 |
| Multi-device bridge (one bridge → N devices) | 1 day | P2 |
| Widget groups (collapsible sections) | 0.5 day | P2 |
| Device dashboard view (dedicated device overview page) | 2-3 days | P2 |
| Custom icons for device types | 0.5 day | P2 |
| Bridge marketplace / registry concept | Design only | P3 |

### Phase 4: Ecosystem

**Goal**: Community-driven bridge ecosystem.

| Task | Effort | Priority |
|------|--------|----------|
| Bridge template generator (CLI tool) | 1-2 days | P2 |
| C/Arduino bridge SDK (for ESP32) | 2-3 days | P2 |
| CDAP protocol documentation site | 1-2 days | P2 |
| Reference bridges (Modbus, SNMP, MQTT, REST) | 3-5 days | P2 |
| OS Agent reference implementation (Linux) | 3-5 days | P2 |
| OS Agent reference implementation (Windows) | 3-5 days | P2 |
| Bridge auto-discovery (mDNS/SSDP) | 1-2 days | P3 |
| Binary protocol option (MessagePack) | 1-2 days | P3 |

### Phase 5: Media Channel (Native Yomie Client)

**Goal**: Full remote desktop capability over CDAP — enabling a native Yomie client.

| Task | Effort | Priority |
|------|--------|----------|
| Media frame mux/demux on CDAP Gateway | 2-3 days | P0 |
| Binary frame relay (server-side, E2E opaque) | 1-2 days | P0 |
| Media session establishment (connect API + pairing) | 1-2 days | P0 |
| E2E key exchange (X25519 → XSalsa20-Poly1305) | 1-2 days | P0 |
| Video channel: codec negotiation + keyframe requests | 1 day | P0 |
| Audio channel: Opus encode/decode + jitter buffer | 1-2 days | P0 |
| Input channel: keyboard/mouse event injection | 1-2 days | P0 |
| Clipboard channel: text/image sync with dedup | 1 day | P1 |
| File transfer channel: chunked, resumable | 1-2 days | P1 |
| Cursor channel: image + hotspot updates | 0.5 day | P1 |
| Panel: WebCodecs/Canvas desktop viewer widget | 3-5 days | P0 |
| Panel: file browser widget (two-pane) | 2-3 days | P1 |
| Panel: video stream widget (camera feeds) | 1-2 days | P1 |
| Desktop agent reference (Rust, Linux/Windows) | 5-10 days | P0 |
| Adaptive quality (bitrate/fps based on acks) | 1-2 days | P1 |
| Multi-monitor support (display index routing) | 1 day | P1 |
| Session recording (server-side, optional) | 2-3 days | P2 |
| P2P media (UDP hole-punch via signal server) | 3-5 days | P2 |

**Deliverables**: Native Yomie desktop agent + panel viewer that fully replaces RustDesk client for managed devices, with remote desktop + system management in one tool.

---

## Comparison with Alternatives

| Feature | Yomie CDAP | Node-RED | Grafana + IoT | Home Assistant | Custom SCADA |
|---------|----------------|----------|---------------|---------------|--------------|
| Remote Desktop | ✅ Native | ❌ | ❌ | ❌ | ❌ |
| Device Widgets | ✅ Declarative | ✅ Flow-based | ✅ Dashboard | ✅ Lovelace | ✅ Custom HMI |
| Bridge Complexity | ~100 LOC | Node config | Agent + config | Integration + YAML | 1000s LOC |
| SCADA Integration | ✅ Via bridges | ✅ Native | ⚠️ Plugin | ⚠️ Plugin | ✅ Native |
| OS Management | ✅ Via OS Agent | ❌ | ❌ | ❌ | ❌ |
| Multi-Tenant | ✅ Existing RBAC | ⚠️ Limited | ✅ Orgs | ❌ | ⚠️ Varies |
| Unified Device List | ✅ Desktops + IoT + SCADA | ❌ | ❌ | ✅ IoT only | ❌ |
| Protocol | JSON/WS (simple) | MQTT/HTTP | Various | Various | Proprietary |
| Self-Hosted | ✅ | ✅ | ✅ | ✅ | ✅ |
| Open Source | ✅ | ✅ | ✅ (core) | ✅ | ❌ Usually |

**Yomie CDAP unique value**: Only platform that manages remote desktops AND industrial/IoT devices in a single panel with unified identity, permissions, and audit trail. With the media channel, CDAP enables a **native Yomie client** that combines remote desktop, system monitoring, file management, and custom integrations in a single agent — something no other platform offers.

---

## FAQ

### Q: Do I need to modify Yomie server to add a new device type?

**No.** Device types are defined by the bridge manifest. The server renders widgets dynamically based on the manifest JSON. Adding a new device type (e.g., "weather_station") requires only writing a bridge — zero server changes.

### Q: Can one bridge manage multiple devices?

**Yes.** A single bridge process can register multiple devices by opening multiple WebSocket connections (one per device) or by using a planned multi-device registration extension.

### Q: What happens if the bridge disconnects?

The device shows as "Offline" in the panel (same as RustDesk clients). Widget values freeze at last known state with a "stale" indicator. When the bridge reconnects, it re-authenticates and re-registers — the server matches by serial number and preserves the same device ID.

### Q: Can CDAP devices appear in the same lists/filters as RustDesk desktops?

**Yes.** Both are stored in the `peers` table with a `device_type` column. Existing search, tags, notes, group assignment, ban/delete — all work for CDAP devices. Dashboard counters separate by type.

### Q: Is CDAP suitable for safety-critical SCADA systems?

**CDAP is a monitoring and convenience control layer, not a safety system.** All safety-critical functions (emergency stops, pressure relief, overcurrent protection) must be implemented in local hardware/PLC logic that operates independently of any network connection. CDAP commands are "requests" — the bridge/device has full authority to reject unsafe operations.

### Q: What's the minimum hardware for running a bridge?

A bridge is a single-process program with minimal resource requirements. An ESP32 (240 MHz, 520 KB RAM) can run a bridge directly. For protocol gateways (Modbus, OPC-UA), a Raspberry Pi or any Linux machine with the protocol library is sufficient.

### Q: Can widget definitions change after registration?

**Yes.** The bridge can send an updated manifest at any time via a `register` message. The server diffs the widget list and updates the panel accordingly. This enables dynamic widget creation (e.g., a PLC that discovers connected modules at runtime).

### Q: Does CDAP replace the RustDesk protocol entirely?

**No — both coexist.** Existing RustDesk clients continue to work unchanged on ports 21115-21119. CDAP runs on port 21122 as a separate gateway. Both device types appear in the same panel, same device list, same permissions system. Migration from RustDesk to native Yomie client is gradual and optional.

### Q: Can CDAP handle remote desktop at 60fps with encryption?

**Yes.** The media channel uses binary WebSocket frames with XSalsa20-Poly1305 encryption (same algorithm as RustDesk). The server only relays opaque encrypted bytes — zero decode overhead. Video encoding/decoding happens at endpoints (hardware-accelerated H.264/VP9). Tested architecture supports 1080p60 at ~3-8 Mbps with <50ms latency through relay.

### Q: What makes the native Yomie client better than RustDesk client?

A Yomie native client is simultaneously a remote desktop tool AND a management agent. Single install provides: remote screen control, file browser, remote terminal, system metrics, service management, custom widgets — all through one protocol, one port, one auth. RustDesk is pure remote desktop; Yomie native client is remote desktop + device management.

### Q: Can I stream a camera feed without allowing input control?

**Yes.** Use the `video_stream` widget type instead of `desktop`. It opens a media session with video-only (and optionally audio) but no input channel. The panel renders a read-only video player with optional snapshot/record buttons.

### Q: Is the media channel end-to-end encrypted?

**Yes for `remote_desktop` capability — mandatory.** The server relays binary frames without decrypting them. Key exchange uses ephemeral X25519 keypairs negotiated between viewer and device. Each frame is encrypted with XSalsa20-Poly1305 using a counter-based nonce. Even the server operator cannot see the screen content.

---

## Appendix A: Reserved Port Allocation

| Port | Service | Status |
|------|---------|--------|
| 21114 | HTTP API (Go server) | ✅ In use |
| 21115 | NAT test | ✅ In use |
| 21116 | Signal (TCP/UDP) | ✅ In use |
| 21117 | Relay (TCP) | ✅ In use |
| 21118 | WebSocket Signal | ✅ In use |
| 21119 | WebSocket Relay | ✅ In use |
| 21120 | HTTP API (alternative) | ✅ In use |
| 21121 | RustDesk Client API (Node.js) | ✅ In use |
| **21122** | **CDAP Gateway (WebSocket + Binary Media)** | **📋 Reserved for CDAP** |
| 21123-21130 | Reserved for future CDAP extensions (P2P, clustering) | 📋 Reserved |

## Appendix B: Wire Protocol Quick Reference

```
Client → Server:
  auth            → {api_key}
  register        → {manifest}
  heartbeat       → {widget_values}
  state_update    → {widget_id, value}
  bulk_update     → {updates: [{widget_id, value}]}
  command_response→ {command_id, status, result}
  event           → {event_type, data}
  log             → {level, message}

Server → Client:
  auth_result     → {success, device_id}
  registered      → {device_id}
  command         → {command_id, widget_id, action, value}
  media_connect   → {session_id, viewer_pk, codecs}
  config_update   → {key, value}
  ping            → {server_time}
  error           → {code, message}

Client → Server (media accept):
  media_accept    → {session_id, device_pk}

Binary Frames (media channel, channel IDs):
  0x01  Video     → codec_id(1B) + pts(8B) + display(1B) + coded_data
  0x02  Audio     → codec_id(1B) + pts(8B) + opus_data
  0x03  Input     → type(1B) + event_data (kbd/mouse/touch)
  0x04  Clipboard → format + hash + raw_bytes
  0x05  File      → transfer_id(4B) + offset(8B) + total(8B) + flags(1B) + data
  0x06  Cursor    → hot_x(2B) + hot_y(2B) + w(2B) + h(2B) + rgba_pixels
  0x07  Display   → display_info (resolution, name, DPI)
  0x08  Control   → JSON media control (keyframe_request, video_ack, quality)
```

## Appendix C: Error Codes

| Code | Name | Description |
|------|------|-------------|
| 1000 | `AUTH_FAILED` | Invalid API key or expired session |
| 1001 | `AUTH_REVOKED` | API key revoked by admin |
| 2000 | `MANIFEST_INVALID` | Manifest JSON validation failed |
| 2001 | `MANIFEST_VERSION_UNSUPPORTED` | Server does not support this manifest version |
| 2002 | `WIDGET_TYPE_UNKNOWN` | Unknown widget type in manifest |
| 3000 | `COMMAND_REJECTED` | Server rejected command (RBAC, rate limit) |
| 3001 | `COMMAND_TIMEOUT` | Device did not respond within timeout |
| 3002 | `DEVICE_OFFLINE` | Target device is not connected |
| 4000 | `RATE_LIMIT` | Too many messages from this bridge |
| 4001 | `MESSAGE_TOO_LARGE` | Message exceeds max size |
| 5000 | `INTERNAL_ERROR` | Server internal error |
| 6000 | `MEDIA_NOT_SUPPORTED` | Device does not declare media capabilities |
| 6001 | `MEDIA_SESSION_FAILED` | Media session establishment failed |
| 6002 | `MEDIA_CODEC_MISMATCH` | No common codec between viewer and device |
| 6003 | `MEDIA_ENCRYPTION_FAILED` | E2E key exchange failed |
| 6004 | `MEDIA_SESSION_LIMIT` | Max concurrent media sessions reached |
| 6005 | `MEDIA_CHANNEL_CLOSED` | Media channel closed by peer |

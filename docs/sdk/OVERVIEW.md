# Yomie SDK Overview

The Yomie SDK enables developers to build custom CDAP agents and bridges that
integrate with the Yomie management platform.

## Available SDKs

| SDK | Language | Package | Directory |
|-----|----------|---------|-----------|
| Python | Python 3.8+ | `yomie-cdap` | `sdks/python/` |
| Node.js | Node.js 18+ | `yomie-cdap` | `sdks/nodejs/` |

## Architecture

```
Your Application Code
        │
        ▼
┌─────────────────┐
│  Yomie SDK │    CDAPBridge class handles:
│  (Python/Node)  │    - WebSocket connection management
│                 │    - Authentication
│  CDAPBridge     │    - Manifest registration
│  Widget         │    - Heartbeat loop
│  Message        │    - Reconnection with backoff
└────────┬────────┘    - Message serialization
         │
    WebSocket
         │
         ▼
┌─────────────────┐
│  Yomie     │
│  Go Server      │
│  CDAP Gateway   │
│  (:21122/cdap)  │
└─────────────────┘
```

## Key Concepts

### CDAPBridge

The main class. Manages the WebSocket connection, authentication, and message loop.
Override methods like `on_connected()`, `on_command()`, and `collect_metrics()` to
implement your agent logic.

### Widget

Widgets define the UI elements that appear in the Yomie web console for your device.
Each widget has a type (gauge, toggle, button, etc.), an ID, and configuration.
Use factory methods: `Widget.gauge()`, `Widget.toggle()`, `Widget.button()`, etc.

### Capabilities

Capabilities declare what your agent can do. Only declare capabilities you actually support.
The server uses capabilities to enable/disable UI features for your device.

## Getting Started

1. Choose your language (Python or Node.js)
2. Install the SDK package
3. Create a CDAPBridge subclass (Python) or instance (Node.js)
4. Define widgets for your device
5. Implement command handlers
6. Connect and run

See language-specific guides:
- [Python SDK Reference](PYTHON_SDK.md)
- [Node.js SDK Reference](NODEJS_SDK.md)
- [Integration Examples](EXAMPLES.md)

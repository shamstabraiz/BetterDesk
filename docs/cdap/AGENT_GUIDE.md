# Building a CDAP Agent

This guide walks through building a custom CDAP agent that connects to a Yomie server.

## Prerequisites

- Yomie server running with CDAP enabled (`--cdap` flag or `CDAP_ENABLED=true`)
- API key created via `POST /api/keys` or the web console
- WebSocket library for your language

## Quick Start (Go)

The reference agent is at `yomie-agent/`. It demonstrates the full lifecycle:

```bash
cd yomie-agent
go build -o yomie-agent .
./yomie-agent \
    --server ws://your-server:21122/cdap \
    --auth-method api_key \
    --api-key YOUR_KEY \
    --device-id MY-DEVICE-001 \
    --device-name "Office Workstation" \
    --device-type os_agent
```

## Quick Start (Python)

Using the Yomie Python SDK:

```bash
pip install yomie-cdap   # or: pip install -e sdks/python/
```

```python
import asyncio
from yomie_cdap import CDAPBridge, Widget

class MyAgent(CDAPBridge):
    def __init__(self):
        super().__init__(
            server_url="ws://your-server:21122/cdap",
            device_id="MY-PY-AGENT",
            device_name="Python Agent",
            device_type="custom_agent",
            auth_method="api_key",
            api_key="YOUR_KEY"
        )
        # Define widgets
        self.add_widget(Widget.gauge("temp", "Temperature", group="Sensors",
                                      unit="°C", min_val=0, max_val=100,
                                      danger=80, warning=60))
        self.add_widget(Widget.button("restart", "Restart Service",
                                       group="Actions", command="restart_svc",
                                       confirm=True))

    async def on_connected(self):
        print("Connected to server!")

    async def on_command(self, command_id, command, args):
        if command == "restart_svc":
            # Execute the command
            import subprocess
            subprocess.run(["systemctl", "restart", "myservice"])
            await self.send_command_response(command_id, True, "Service restarted")
        else:
            await self.send_command_response(command_id, False, f"Unknown: {command}")

    async def collect_metrics(self):
        """Called every heartbeat interval."""
        import psutil
        return {
            "temp": read_temperature_sensor(),
        }

asyncio.run(MyAgent().run())
```

## Quick Start (Node.js)

```bash
npm install yomie-cdap   # or: npm install ./sdks/nodejs/
```

```javascript
const { CDAPBridge, Widget } = require('yomie-cdap');

const agent = new CDAPBridge({
    serverUrl: 'ws://your-server:21122/cdap',
    deviceId: 'MY-NODE-AGENT',
    deviceName: 'Node.js Agent',
    deviceType: 'custom_agent',
    authMethod: 'api_key',
    apiKey: 'YOUR_KEY'
});

agent.addWidget(Widget.gauge('cpu', 'CPU Usage', {
    group: 'System', unit: '%', min: 0, max: 100, danger: 90
}));

agent.on('command', async (cmdId, command, args) => {
    if (command === 'ping') {
        agent.sendCommandResponse(cmdId, true, 'pong');
    }
});

agent.on('connected', () => console.log('Connected!'));

agent.connect();
```

## Agent Lifecycle

1. **Connect** — Establish WebSocket to `ws://server:21122/cdap`
2. **Authenticate** — Send `auth` message, wait for `auth_response`
3. **Register manifest** — Send device info, capabilities, and widget definitions
4. **Heartbeat loop** — Periodically send metrics and widget values
5. **Handle commands** — Listen for `command` messages, execute, respond
6. **Reconnect** — On disconnect, exponential backoff reconnect (2s, 4s, 8s... max 60s)

## Widget Best Practices

- Use **gauge** for numeric metrics with known ranges (CPU, temperature, battery)
- Use **toggle** for on/off controls (enable/disable service, relay switch)
- Use **button** with `confirm: true` for destructive actions (reboot, clear data)
- Use **text** for string values (hostname, IP address, firmware version)
- Group related widgets with the `group` field for organized display
- Keep widget IDs stable across reconnects (don't generate random IDs)

## Capabilities

Only declare capabilities your agent actually supports:

| Capability | When to declare |
|------------|----------------|
| `telemetry` | Agent collects and reports system/sensor metrics |
| `commands` | Agent can execute named commands |
| `terminal` | Agent provides PTY terminal access |
| `file_transfer` | Agent supports file browse/read/write/delete |
| `clipboard` | Agent can get/set system clipboard |
| `screenshot` | Agent can capture screen |
| `audio` | Agent supports audio streaming |

## Security Notes

- **Never hardcode API keys** in source code. Use environment variables or config files.
- **Validate all commands** before executing. Reject unknown commands.
- **Path traversal protection** — Validate file paths in file_transfer operations. The reference agent uses `safePath()` to prevent `../../` attacks.
- **Rate limiting** — Don't flood the server with state updates. Batch changes with `bulk_update`.

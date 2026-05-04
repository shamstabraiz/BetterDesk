# Python SDK Reference — `yomie-cdap`

## Installation

```bash
pip install yomie-cdap
# or from source:
pip install -e sdks/python/
```

**Requirements:** Python 3.8+, `websockets >= 12.0`

## CDAPBridge

The main class for building CDAP agents and bridges.

```python
from yomie_cdap import CDAPBridge, Widget
```

### Constructor

```python
CDAPBridge(
    server_url: str,          # WebSocket URL (ws://host:21122/cdap)
    device_id: str,           # Unique device identifier
    device_name: str,         # Human-readable name
    device_type: str,         # Device type (os_agent, sensor, plc, etc.)
    auth_method: str = "api_key",  # api_key | device_token | user_password
    api_key: str = None,      # API key (when auth_method=api_key)
    capabilities: list = None, # ["telemetry", "commands", ...]
    heartbeat_sec: int = 15,  # Heartbeat interval in seconds
    reconnect_sec: int = 5,   # Base reconnect delay
)
```

### Methods

| Method | Description |
|--------|-------------|
| `add_widget(widget)` | Add a Widget to the device manifest |
| `async run()` | Connect and run the main loop (blocks) |
| `async update_widget(id, value)` | Update a single widget value |
| `async bulk_update(values: dict)` | Update multiple widget values at once |
| `async send_command_response(cmd_id, success, result)` | Respond to a command |
| `async send_alert(severity, message, source)` | Send an alert to the server |

### Override Methods

```python
class MyAgent(CDAPBridge):
    async def on_connected(self):
        """Called after successful auth + manifest registration."""
        pass

    async def on_disconnected(self):
        """Called when WebSocket closes."""
        pass

    async def on_command(self, command_id, command, args):
        """Called when server sends a command."""
        pass

    async def collect_metrics(self):
        """Called every heartbeat. Return dict of widget_id: value."""
        return {}
```

## Widget

Factory class for creating CDAP widgets.

### Factory Methods

```python
Widget.gauge(id, label, group="", unit="", min_val=0, max_val=100,
             danger=None, warning=None)

Widget.toggle(id, label, group="", on_command="", off_command="")

Widget.button(id, label, group="", command="", confirm=False)

Widget.led(id, label, group="", on_color="#3fb950", off_color="#f85149")

Widget.text(id, label, group="")

Widget.slider(id, label, group="", min_val=0, max_val=100,
              step=1, command="")

Widget.select(id, label, group="", options=[], command="")

Widget.chart(id, label, group="", max_points=60)
```

### Example

```python
widgets = [
    Widget.gauge("cpu", "CPU Usage", group="System", unit="%",
                 min_val=0, max_val=100, danger=90, warning=70),
    Widget.text("hostname", "Hostname", group="System"),
    Widget.toggle("relay", "Power Relay", group="Control",
                  on_command="relay_on", off_command="relay_off"),
    Widget.button("reboot", "Reboot", group="Actions",
                  command="system_reboot", confirm=True),
]
```

## Message

Internal message class. Normally not used directly.

```python
from yomie_cdap import Message

msg = Message(type="custom", payload={"key": "value"})
```

## Complete Example

```python
import asyncio
import os
from yomie_cdap import CDAPBridge, Widget

class ServerMonitor(CDAPBridge):
    def __init__(self):
        super().__init__(
            server_url=os.environ.get("CDAP_SERVER", "ws://localhost:21122/cdap"),
            device_id="MONITOR-001",
            device_name="Production Server",
            device_type="os_agent",
            auth_method="api_key",
            api_key=os.environ["CDAP_API_KEY"],
            capabilities=["telemetry", "commands"]
        )
        self.add_widget(Widget.gauge("cpu", "CPU", group="System",
                                      unit="%", max_val=100, danger=90))
        self.add_widget(Widget.gauge("ram", "Memory", group="System",
                                      unit="%", max_val=100, danger=85))
        self.add_widget(Widget.gauge("disk", "Disk", group="System",
                                      unit="%", max_val=100, danger=95))
        self.add_widget(Widget.text("uptime", "Uptime", group="Info"))

    async def collect_metrics(self):
        import psutil
        return {
            "cpu": psutil.cpu_percent(interval=1),
            "ram": psutil.virtual_memory().percent,
            "disk": psutil.disk_usage('/').percent,
            "uptime": str(datetime.timedelta(seconds=int(time.time() - psutil.boot_time())))
        }

    async def on_command(self, cmd_id, command, args):
        if command == "get_processes":
            procs = [p.info for p in psutil.process_iter(['pid', 'name', 'cpu_percent'])]
            await self.send_command_response(cmd_id, True, procs[:20])
        else:
            await self.send_command_response(cmd_id, False, f"Unknown: {command}")

if __name__ == "__main__":
    asyncio.run(ServerMonitor().run())
```

# Building a CDAP Bridge

A CDAP bridge connects non-Yomie devices (industrial equipment, IoT sensors, network gear)
to the Yomie management platform via the CDAP protocol.

## What is a Bridge?

A bridge is a lightweight process that:
1. Communicates with external hardware/software using its native protocol (Modbus, SNMP, REST, MQTT, etc.)
2. Translates data into CDAP widget values
3. Exposes CDAP commands that map to hardware actions

```
┌──────────────┐     Modbus/TCP     ┌──────────┐     CDAP/WS     ┌──────────────┐
│  PLC / RTU   │ ◄═══════════════► │  Bridge  │ ◄═════════════► │  Yomie  │
│  (hardware)  │                    │  (Python) │                 │  Server      │
└──────────────┘                    └──────────┘                  └──────────────┘
```

## Reference Bridges

| Bridge | Protocol | Directory | Description |
|--------|----------|-----------|-------------|
| Modbus | Modbus TCP/RTU | `bridges/modbus/` | Register polling, data type encode/decode, write-back |
| SNMP | SNMPv2c/v3 | `bridges/snmp/` | OID polling, timetick formatting, counter rates |
| REST/Webhook | HTTP | `bridges/rest-webhook/` | REST polling + aiohttp webhook listener |

## Creating a Bridge (Python)

### 1. Install the SDK

```bash
pip install yomie-cdap   # or: pip install -e sdks/python/
```

### 2. Define Your Bridge

```python
import asyncio
from betterdesk_cdap import CDAPBridge, Widget

class TemperatureBridge(CDAPBridge):
    """Bridge for a network temperature sensor."""

    def __init__(self, sensor_ip, sensor_port=502):
        super().__init__(
            server_url="ws://yomie-server:21122/cdap",
            device_id=f"TEMP-{sensor_ip.replace('.', '-')}",
            device_name=f"Temp Sensor {sensor_ip}",
            device_type="temperature_sensor",
            auth_method="api_key",
            api_key=os.environ["CDAP_API_KEY"]
        )
        self.sensor_ip = sensor_ip
        self.sensor_port = sensor_port

        # Define widgets
        self.add_widget(Widget.gauge("temp", "Temperature", group="Readings",
                                      unit="°C", min_val=-40, max_val=125,
                                      danger=80, warning=60))
        self.add_widget(Widget.text("firmware", "Firmware", group="Info"))
        self.add_widget(Widget.led("alarm", "High Temp Alarm", group="Status"))

    async def on_connected(self):
        # Read firmware version once
        fw = await self._read_firmware()
        await self.update_widget("firmware", fw)

    async def collect_metrics(self):
        """Called every heartbeat interval. Read sensor data."""
        temp = await self._read_temperature()
        alarm = temp > 80
        return {
            "temp": round(temp, 1),
            "alarm": alarm,
        }

    async def on_command(self, command_id, command, args):
        if command == "set_threshold":
            threshold = args.get("value", 80)
            await self._write_threshold(threshold)
            await self.send_command_response(command_id, True, f"Threshold set to {threshold}")
        else:
            await self.send_command_response(command_id, False, f"Unknown command: {command}")

    async def _read_temperature(self):
        # Your hardware-specific code here
        ...

asyncio.run(TemperatureBridge("192.168.1.100").run())
```

### 3. Configuration

Use a JSON or YAML config file:

```json
{
    "server_url": "ws://yomie:21122/cdap",
    "api_key": "${CDAP_API_KEY}",
    "device_id": "MODBUS-PLC-001",
    "device_name": "Production Line PLC",
    "device_type": "modbus_plc",
    "poll_interval": 5,
    "registers": [
        { "address": 0, "type": "float32", "widget_id": "pressure", "label": "Pressure", "unit": "bar" },
        { "address": 2, "type": "uint16", "widget_id": "rpm", "label": "Motor RPM", "unit": "RPM" }
    ]
}
```

## Bridge Best Practices

1. **Reconnection** — The SDK handles WebSocket reconnection with exponential backoff. Don't implement your own retry logic.
2. **Error isolation** — Catch hardware communication errors in `collect_metrics()`. Return last known values on failure.
3. **Poll intervals** — Match the bridge's poll interval to the hardware's update rate. Don't poll a 1Hz sensor at 100ms.
4. **Bulk updates** — Use `bulk_update` to send all widget values at once, reducing WebSocket messages.
5. **Device IDs** — Use deterministic IDs based on hardware address (IP, serial number). Don't use random UUIDs.
6. **Graceful shutdown** — Handle `SIGINT`/`SIGTERM` to disconnect cleanly from both hardware and server.

## Node.js Bridge

```javascript
const { CDAPBridge, Widget } = require('yomie-cdap');

const bridge = new CDAPBridge({
    serverUrl: 'ws://yomie:21122/cdap',
    deviceId: 'REST-SENSOR-001',
    deviceName: 'REST API Sensor',
    deviceType: 'rest_sensor',
    authMethod: 'api_key',
    apiKey: process.env.CDAP_API_KEY,
    heartbeatSec: 10
});

bridge.addWidget(Widget.gauge('humidity', 'Humidity', {
    group: 'Environment', unit: '%', min: 0, max: 100
}));

bridge.on('collectMetrics', async () => {
    const resp = await fetch('http://sensor-device/api/data');
    const data = await resp.json();
    return { humidity: data.humidity };
});

bridge.connect();
```

## Deployment

Bridges are typically deployed as:
- **systemd services** on Linux
- **Docker containers** alongside the Yomie stack
- **NSSM services** on Windows

See `bridges/modbus/Dockerfile` and `bridges/modbus/bridge.service` for examples.

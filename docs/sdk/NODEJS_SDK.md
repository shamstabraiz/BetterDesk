# Node.js SDK Reference — `yomie-cdap`

## Installation

```bash
npm install yomie-cdap
# or from source:
npm install ./sdks/nodejs/
```

**Requirements:** Node.js 18+, `ws ^8.18.0`

## CDAPBridge

The main class. Extends `EventEmitter`.

```javascript
const { CDAPBridge, Widget } = require('yomie-cdap');
```

### Constructor

```javascript
const bridge = new CDAPBridge({
    serverUrl: 'ws://host:21122/cdap',  // WebSocket URL
    deviceId: 'MY-DEVICE',              // Unique device ID
    deviceName: 'My Device',            // Human-readable name
    deviceType: 'custom_agent',         // Device type
    authMethod: 'api_key',              // api_key | device_token | user_password
    apiKey: 'YOUR_KEY',                 // API key
    capabilities: ['telemetry', 'commands'],
    heartbeatSec: 15,                   // Heartbeat interval
    reconnectSec: 5,                    // Base reconnect delay
});
```

### Methods

| Method | Description |
|--------|-------------|
| `addWidget(widget)` | Add a Widget to the manifest |
| `connect()` | Connect to server (auto-reconnects) |
| `disconnect()` | Gracefully disconnect |
| `updateWidget(id, value)` | Update a single widget value |
| `bulkUpdate(values)` | Update multiple widget values `{ id: value }` |
| `sendCommandResponse(cmdId, success, result)` | Respond to a command |
| `sendAlert(severity, message, source)` | Send an alert |

### Events

```javascript
bridge.on('connected', () => { /* auth + manifest done */ });
bridge.on('disconnected', () => { /* connection lost */ });
bridge.on('command', async (cmdId, command, args) => { /* handle command */ });
bridge.on('collectMetrics', async () => { /* return { id: value } */ });
bridge.on('error', (err) => { /* handle error */ });
```

## Widget

Factory class for creating CDAP widgets.

### Factory Methods

```javascript
Widget.gauge(id, label, { group, unit, min, max, danger, warning })
Widget.toggle(id, label, { group, onCommand, offCommand })
Widget.button(id, label, { group, command, confirm })
Widget.led(id, label, { group, onColor, offColor })
Widget.text(id, label, { group })
Widget.slider(id, label, { group, min, max, step, command })
Widget.select(id, label, { group, options, command })
Widget.chart(id, label, { group, maxPoints })
```

### Example

```javascript
bridge.addWidget(Widget.gauge('cpu', 'CPU Usage', {
    group: 'System', unit: '%', min: 0, max: 100, danger: 90, warning: 70
}));

bridge.addWidget(Widget.text('hostname', 'Hostname', { group: 'Info' }));

bridge.addWidget(Widget.button('restart', 'Restart Service', {
    group: 'Actions', command: 'restart_svc', confirm: true
}));
```

## Complete Example

```javascript
const { CDAPBridge, Widget } = require('yomie-cdap');
const os = require('os');

const bridge = new CDAPBridge({
    serverUrl: process.env.CDAP_SERVER || 'ws://localhost:21122/cdap',
    deviceId: `NODE-${os.hostname().toUpperCase()}`,
    deviceName: os.hostname(),
    deviceType: 'os_agent',
    authMethod: 'api_key',
    apiKey: process.env.CDAP_API_KEY,
    capabilities: ['telemetry', 'commands']
});

bridge.addWidget(Widget.gauge('cpu', 'CPU', { group: 'System', unit: '%', max: 100, danger: 90 }));
bridge.addWidget(Widget.gauge('mem', 'Memory', { group: 'System', unit: '%', max: 100, danger: 85 }));
bridge.addWidget(Widget.text('uptime', 'Uptime', { group: 'Info' }));

bridge.on('connected', () => console.log('Connected to Yomie'));

bridge.on('collectMetrics', () => {
    const cpus = os.cpus();
    const idle = cpus.reduce((sum, c) => sum + c.times.idle, 0) / cpus.length;
    const total = cpus.reduce((sum, c) => sum + Object.values(c.times).reduce((a, b) => a + b), 0) / cpus.length;
    const freeMem = os.freemem();
    const totalMem = os.totalmem();

    return {
        cpu: Math.round((1 - idle / total) * 100),
        mem: Math.round(((totalMem - freeMem) / totalMem) * 100),
        uptime: `${Math.floor(os.uptime() / 3600)}h ${Math.floor((os.uptime() % 3600) / 60)}m`
    };
});

bridge.on('command', (cmdId, command, args) => {
    if (command === 'get_hostname') {
        bridge.sendCommandResponse(cmdId, true, os.hostname());
    } else {
        bridge.sendCommandResponse(cmdId, false, `Unknown command: ${command}`);
    }
});

bridge.connect();
```

## Error Handling

```javascript
bridge.on('error', (err) => {
    console.error('Bridge error:', err.message);
    // SDK handles reconnection automatically
});

// For unhandled command errors
bridge.on('command', async (cmdId, command, args) => {
    try {
        // ... your logic
    } catch (err) {
        bridge.sendCommandResponse(cmdId, false, err.message);
    }
});
```

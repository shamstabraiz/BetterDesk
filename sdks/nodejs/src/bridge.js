'use strict';

const EventEmitter = require('events');
const WebSocket = require('ws');
const os = require('os');
const { createMessage, parseMessage, authApiKey, authDeviceToken, authUserPassword } = require('./protocol');

/**
 * CDAPBridge — connects a Node.js process to Yomie CDAP Gateway.
 *
 * @fires CDAPBridge#connected
 * @fires CDAPBridge#disconnected
 * @fires CDAPBridge#registered
 * @fires CDAPBridge#command
 * @fires CDAPBridge#error
 * @fires CDAPBridge#message
 */
class CDAPBridge extends EventEmitter {
  /**
   * @param {object} options
   * @param {string} options.server - WebSocket URL, e.g. ws://host:21122/cdap
   * @param {string} [options.authMethod='api_key'] - api_key | device_token | user_password
   * @param {string} [options.apiKey]
   * @param {string} [options.deviceToken]
   * @param {string} [options.username]
   * @param {string} [options.password]
   * @param {string} [options.deviceId]
   * @param {string} [options.deviceName]
   * @param {string} [options.deviceType='bridge']
   * @param {string} [options.bridgeName]
   * @param {string} [options.bridgeVersion='1.0.0']
   * @param {number} [options.heartbeatSec=15]
   * @param {number} [options.reconnectSec=5]
   * @param {number} [options.maxReconnect=120]
   */
  constructor(options) {
    super();
    this.server = options.server;
    this.authMethod = options.authMethod || 'api_key';
    this.apiKey = options.apiKey || '';
    this.deviceToken = options.deviceToken || '';
    this.username = options.username || '';
    this.password = options.password || '';
    this.deviceId = options.deviceId || '';
    this.deviceName = options.deviceName || os.hostname();
    this.deviceType = options.deviceType || 'bridge';
    this.bridgeName = options.bridgeName || '';
    this.bridgeVersion = options.bridgeVersion || '1.0.0';
    this.heartbeatSec = options.heartbeatSec || 15;
    this.reconnectSec = options.reconnectSec || 5;
    this.maxReconnect = options.maxReconnect || 120;

    /** @type {Map<string, import('./widgets').Widget>} */
    this._widgets = new Map();
    /** @type {Map<string, any>} */
    this._state = new Map();
    /** @type {Map<string, Function>} */
    this._handlers = new Map();

    this._ws = null;
    this._heartbeatTimer = null;
    this._token = null;
    this._running = false;
    this._backoff = this.reconnectSec;
  }

  // ── Widget registration ──────────────────────────────────────────

  /**
   * Add a widget to the manifest. Must be called before connect().
   * @param {import('./widgets').Widget} widget
   * @returns {this}
   */
  addWidget(widget) {
    this._widgets.set(widget.id, widget);
    return this;
  }

  /**
   * Register a command handler for a specific widget.
   * @param {string} widgetId
   * @param {function(object): any} handler - receives command payload, may return a value
   * @returns {this}
   */
  onCommand(widgetId, handler) {
    this._handlers.set(widgetId, handler);
    return this;
  }

  // ── State updates ────────────────────────────────────────────────

  /**
   * Update a single widget value and push to server.
   * @param {string} widgetId
   * @param {any} value
   */
  updateState(widgetId, value) {
    this._state.set(widgetId, value);
    this._send('state_update', { widget_id: widgetId, value });
  }

  /**
   * Bulk-update multiple widget values.
   * @param {Object<string, any>} updates - { widgetId: value, ... }
   */
  bulkUpdate(updates) {
    const values = {};
    for (const [k, v] of Object.entries(updates)) {
      this._state.set(k, v);
      values[k] = v;
    }
    this._send('state_update', { values });
  }

  // ── Alerts & logging ─────────────────────────────────────────────

  /**
   * Fire an alert.
   * @param {string} alertId
   * @param {string} severity - critical | warning | info
   * @param {string} message
   * @param {object} [details]
   */
  fireAlert(alertId, severity, message, details) {
    this._send('alert', { alert_id: alertId, severity, message, details: details || {} });
  }

  /**
   * Resolve a previously fired alert.
   * @param {string} alertId
   */
  resolveAlert(alertId) {
    this._send('alert_resolve', { alert_id: alertId });
  }

  /**
   * Send a log entry.
   * @param {string} level - debug | info | warn | error
   * @param {string} message
   * @param {object} [data]
   */
  sendLog(level, message, data) {
    this._send('log', { level, message, data: data || {} });
  }

  // ── Connection lifecycle ─────────────────────────────────────────

  /**
   * Start the bridge — connects, authenticates, registers, and maintains heartbeat.
   * Reconnects automatically on disconnection.
   * @returns {Promise<void>} resolves when stop() is called
   */
  async run() {
    this._running = true;
    this._backoff = this.reconnectSec;

    while (this._running) {
      try {
        await this._connectOnce();
        // If we get here, connection was cleanly closed
      } catch (err) {
        if (!this._running) break;
        this.emit('error', err);
      }

      if (!this._running) break;

      // Exponential backoff with jitter
      const jitter = Math.floor(Math.random() * (this._backoff / 4));
      const delay = this._backoff + jitter;
      await this._sleep(delay * 1000);
      this._backoff = Math.min(this._backoff * 2, this.maxReconnect);
    }
  }

  /**
   * Stop the bridge gracefully.
   */
  stop() {
    this._running = false;
    this._clearHeartbeat();
    if (this._ws && this._ws.readyState === WebSocket.OPEN) {
      this._ws.close(1000, 'bridge stopping');
    }
    this._ws = null;
  }

  // ── Internal ─────────────────────────────────────────────────────

  /** @private */
  async _connectOnce() {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(this.server, 'cdap', {
        headers: { 'User-Agent': `yomie-cdap-nodejs/${this.bridgeVersion}` },
        handshakeTimeout: 10000,
      });

      let authenticated = false;

      ws.on('open', () => {
        this._ws = ws;
        this._backoff = this.reconnectSec; // reset backoff on success
        this.emit('connected');
        this._authenticate();
      });

      ws.on('message', (raw) => {
        let msg;
        try {
          msg = parseMessage(raw);
        } catch {
          return;
        }

        this.emit('message', msg);

        switch (msg.type) {
          case 'auth_result':
            if (msg.payload.success) {
              authenticated = true;
              this._token = msg.payload.token || this._token;
              if (msg.payload.device_id) this.deviceId = msg.payload.device_id;
              this._register();
            } else {
              ws.close(4001, 'auth failed');
              reject(new Error(`Auth failed: ${msg.payload.error || 'unknown'}`));
            }
            break;

          case 'registered':
            this.emit('registered', msg.payload);
            this._startHeartbeat();
            break;

          case 'command':
            this._handleCommand(msg);
            break;

          case 'heartbeat_ack':
            // noop
            break;

          case 'error':
            this.emit('error', new Error(msg.payload.message || 'server error'));
            break;

          default:
            // Forward unrecognized message types via event
            this.emit(msg.type, msg.payload);
            break;
        }
      });

      ws.on('close', (code, reason) => {
        this._clearHeartbeat();
        this._ws = null;
        this.emit('disconnected', { code, reason: reason.toString() });
        if (authenticated) {
          resolve(); // normal reconnect
        } else if (code !== 4001) {
          resolve();
        }
      });

      ws.on('error', (err) => {
        // error fires before close, let close handler resolve/reject
        if (!authenticated && !ws._closeCalled) {
          reject(err);
        }
      });
    });
  }

  /** @private */
  _authenticate() {
    let payload;
    switch (this.authMethod) {
      case 'device_token':
        payload = authDeviceToken(this.deviceToken, this.deviceId, this.bridgeVersion);
        break;
      case 'user_password':
        payload = authUserPassword(this.username, this.password, this.deviceId, this.bridgeVersion);
        break;
      default:
        payload = authApiKey(this.apiKey, this.deviceId, this.bridgeVersion);
        break;
    }
    this._send('auth', payload);
  }

  /** @private */
  _register() {
    const widgets = [];
    for (const w of this._widgets.values()) {
      widgets.push(typeof w.toJSON === 'function' ? w.toJSON() : w);
    }

    const capabilities = ['telemetry', 'commands', 'alerts', 'logs'];
    const manifest = {
      manifest_version: '1.0',
      device: {
        id: this.deviceId,
        name: this.deviceName,
        type: this.deviceType,
        firmware_version: this.bridgeVersion,
        hostname: os.hostname(),
        os: `${os.type()} ${os.release()}`,
        platform: os.platform(),
      },
      capabilities,
      widgets,
      heartbeat_interval: this.heartbeatSec,
    };

    if (this.bridgeName) {
      manifest.bridge = { name: this.bridgeName, version: this.bridgeVersion };
    }

    this._send('register', manifest);
  }

  /** @private */
  _startHeartbeat() {
    this._clearHeartbeat();
    this._heartbeatTimer = setInterval(() => {
      const widgetValues = {};
      for (const [k, v] of this._state) {
        widgetValues[k] = v;
      }
      this._send('heartbeat', {
        uptime: Math.floor(process.uptime()),
        widget_values: widgetValues,
      });
    }, this.heartbeatSec * 1000);
  }

  /** @private */
  _clearHeartbeat() {
    if (this._heartbeatTimer) {
      clearInterval(this._heartbeatTimer);
      this._heartbeatTimer = null;
    }
  }

  /** @private */
  _handleCommand(msg) {
    const { widget_id, action, value, params } = msg.payload;
    const handler = this._handlers.get(widget_id);

    this.emit('command', msg.payload);

    if (!handler) {
      this._send('command_response', {
        id: msg.id,
        widget_id,
        success: false,
        error: `No handler for widget ${widget_id}`,
      });
      return;
    }

    const start = Date.now();
    try {
      const result = handler({ widget_id, action, value, params });

      // Handle both sync and async handlers
      if (result && typeof result.then === 'function') {
        result
          .then((val) => {
            this._sendCommandResponse(msg.id, widget_id, true, val, Date.now() - start);
            if (val !== undefined) {
              this._state.set(widget_id, val);
            }
          })
          .catch((err) => {
            this._sendCommandResponse(msg.id, widget_id, false, null, Date.now() - start, err.message);
          });
      } else {
        this._sendCommandResponse(msg.id, widget_id, true, result, Date.now() - start);
        if (result !== undefined) {
          this._state.set(widget_id, result);
        }
      }
    } catch (err) {
      this._sendCommandResponse(msg.id, widget_id, false, null, Date.now() - start, err.message);
    }
  }

  /** @private */
  _sendCommandResponse(id, widgetId, success, value, durationMs, error) {
    const payload = { id, widget_id: widgetId, success, duration_ms: durationMs };
    if (value !== undefined && value !== null) payload.value = value;
    if (error) payload.error = error;
    this._send('command_response', payload);
  }

  /** @private */
  _send(type, payload) {
    if (this._ws && this._ws.readyState === WebSocket.OPEN) {
      this._ws.send(createMessage(type, payload));
    }
  }

  /** @private */
  _sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }
}

module.exports = { CDAPBridge };

#!/usr/bin/env bash
# Yomie Agent — Linux installer (systemd)
# Usage: sudo ./install.sh [OPTIONS]
#   -s URL        Gateway WebSocket URL
#   -k KEY        API key
#   -n NAME       Device name
#   -d DIR        Install directory (default: /opt/yomie-agent)
#   -u            Uninstall
set -euo pipefail

INSTALL_DIR="/opt/yomie-agent"
SERVICE_NAME="yomie-agent"
USER_NAME="yomie-agent"
CONFIG_FILE=""
SERVER_URL=""
API_KEY=""
DEVICE_NAME=""
UNINSTALL=false

usage() {
    echo "Usage: sudo $0 [-s URL] [-k KEY] [-n NAME] [-d DIR] [-u]"
    echo "  -s URL   Gateway WebSocket URL (ws://host:21122/cdap)"
    echo "  -k KEY   API key for authentication"
    echo "  -n NAME  Device name (default: hostname)"
    echo "  -d DIR   Install directory (default: /opt/yomie-agent)"
    echo "  -u       Uninstall"
    exit 1
}

while getopts "s:k:n:d:uh" opt; do
    case $opt in
        s) SERVER_URL="$OPTARG" ;;
        k) API_KEY="$OPTARG" ;;
        n) DEVICE_NAME="$OPTARG" ;;
        d) INSTALL_DIR="$OPTARG" ;;
        u) UNINSTALL=true ;;
        h|*) usage ;;
    esac
done

if [ "$(id -u)" -ne 0 ]; then
    echo "ERROR: This script must be run as root (sudo)"
    exit 1
fi

uninstall() {
    echo "=== Uninstalling Yomie Agent ==="
    systemctl stop "$SERVICE_NAME" 2>/dev/null || true
    systemctl disable "$SERVICE_NAME" 2>/dev/null || true
    rm -f "/etc/systemd/system/${SERVICE_NAME}.service"
    systemctl daemon-reload
    if id "$USER_NAME" &>/dev/null; then
        userdel "$USER_NAME" 2>/dev/null || true
    fi
    rm -rf "$INSTALL_DIR"
    echo "Yomie Agent uninstalled."
    exit 0
}

if $UNINSTALL; then
    uninstall
fi

# Detect binary
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BINARY=""
if [ -f "${SCRIPT_DIR}/../yomie-agent-linux-amd64" ] && [ "$(uname -m)" = "x86_64" ]; then
    BINARY="${SCRIPT_DIR}/../yomie-agent-linux-amd64"
elif [ -f "${SCRIPT_DIR}/../yomie-agent" ]; then
    BINARY="${SCRIPT_DIR}/../yomie-agent"
else
    echo "ERROR: Agent binary not found. Build it first: go build -o yomie-agent ."
    exit 1
fi

echo "=== Installing Yomie Agent ==="

# Create service user
if ! id "$USER_NAME" &>/dev/null; then
    useradd --system --no-create-home --shell /usr/sbin/nologin "$USER_NAME"
    echo "Created service user: $USER_NAME"
fi

# Install binary
mkdir -p "$INSTALL_DIR"
cp "$BINARY" "${INSTALL_DIR}/yomie-agent"
chmod 755 "${INSTALL_DIR}/yomie-agent"

# Create data directory
mkdir -p "${INSTALL_DIR}/data"
chown -R "$USER_NAME:$USER_NAME" "${INSTALL_DIR}/data"

# Create config if not exists
CONFIG_FILE="${INSTALL_DIR}/config.json"
if [ ! -f "$CONFIG_FILE" ]; then
    if [ -z "$SERVER_URL" ]; then
        read -rp "Gateway WebSocket URL (ws://host:21122/cdap): " SERVER_URL
    fi
    if [ -z "$API_KEY" ]; then
        read -rp "API Key: " API_KEY
    fi
    if [ -z "$DEVICE_NAME" ]; then
        DEVICE_NAME="$(hostname)"
    fi

    cat > "$CONFIG_FILE" <<JSONEOF
{
  "server": "${SERVER_URL}",
  "auth_method": "api_key",
  "api_key": "${API_KEY}",
  "device_name": "${DEVICE_NAME}",
  "device_type": "os_agent",
  "terminal": true,
  "file_browser": true,
  "clipboard": true,
  "screenshot": true,
  "file_root": "/",
  "heartbeat_sec": 15,
  "reconnect_sec": 5,
  "max_reconnect": 300,
  "log_level": "info",
  "data_dir": "${INSTALL_DIR}/data"
}
JSONEOF
    chmod 600 "$CONFIG_FILE"
    chown "$USER_NAME:$USER_NAME" "$CONFIG_FILE"
    echo "Config created: $CONFIG_FILE"
else
    echo "Config exists, preserving: $CONFIG_FILE"
fi

# Create systemd service
cat > "/etc/systemd/system/${SERVICE_NAME}.service" <<EOF
[Unit]
Description=Yomie CDAP Agent
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=${USER_NAME}
Group=${USER_NAME}
ExecStart=${INSTALL_DIR}/yomie-agent -config ${CONFIG_FILE}
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal
SyslogIdentifier=${SERVICE_NAME}

# Security hardening
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=${INSTALL_DIR}/data
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable "$SERVICE_NAME"
systemctl start "$SERVICE_NAME"

echo ""
echo "=== Yomie Agent Installed ==="
echo "  Binary:  ${INSTALL_DIR}/yomie-agent"
echo "  Config:  ${CONFIG_FILE}"
echo "  Service: ${SERVICE_NAME}"
echo ""
echo "Commands:"
echo "  systemctl status  $SERVICE_NAME"
echo "  journalctl -u $SERVICE_NAME -f"
echo "  systemctl restart $SERVICE_NAME"

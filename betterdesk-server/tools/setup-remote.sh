#!/bin/bash
set -euo pipefail
echo '=== Yomie Go Server Setup ==='

# Directories
mkdir -p /opt/yomie-go/data /opt/yomie-go/backups /var/log/yomie-go

# Backup
TS=$(date +%Y%m%d_%H%M%S)
echo "[1/8] Backup (timestamp: ${TS})..."
cp /opt/rustdesk/db_v2.sqlite3 "/opt/yomie-go/backups/db_v2.sqlite3.${TS}"
[ -f /opt/rustdesk/db_v2.sqlite3-wal ] && cp /opt/rustdesk/db_v2.sqlite3-wal "/opt/yomie-go/backups/db_v2.sqlite3-wal.${TS}" || true
[ -f /opt/rustdesk/db_v2.sqlite3-shm ] && cp /opt/rustdesk/db_v2.sqlite3-shm "/opt/yomie-go/backups/db_v2.sqlite3-shm.${TS}" || true
cp /opt/rustdesk/id_ed25519 "/opt/yomie-go/backups/id_ed25519.${TS}"
cp /opt/rustdesk/id_ed25519.pub "/opt/yomie-go/backups/id_ed25519.pub.${TS}"
echo '    Backup done.'

# Install binary
echo '[2/8] Installing binary...'
cp /tmp/yomie-server-linux-amd64 /opt/yomie-go/yomie-server
chmod +x /opt/yomie-go/yomie-server
cp /tmp/migrate-linux-amd64 /opt/yomie-go/migrate
chmod +x /opt/yomie-go/migrate

# Copy keys
echo '[3/8] Copying Ed25519 keys...'
cp /opt/rustdesk/id_ed25519 /opt/yomie-go/data/
cp /opt/rustdesk/id_ed25519.pub /opt/yomie-go/data/

# Migrate database
echo '[4/8] Migrating database...'
/opt/yomie-go/migrate -src /opt/rustdesk/db_v2.sqlite3 -dst /opt/yomie-go/data/db_v2.sqlite3

# Stop old services
echo '[5/8] Stopping old hbbs/hbbr...'
systemctl stop rustdesksignal 2>/dev/null || true
systemctl stop rustdeskrelay 2>/dev/null || true
systemctl disable rustdesksignal 2>/dev/null || true
systemctl disable rustdeskrelay 2>/dev/null || true

# Create systemd service
echo '[6/8] Creating systemd service...'
cat > /etc/systemd/system/yomie-go.service << 'SVC'
[Unit]
Description=Yomie Go Server (Signal + Relay + API)
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/yomie-go/data
ExecStart=/opt/yomie-go/yomie-server -mode all -db /opt/yomie-go/data/db_v2.sqlite3 -key-file /opt/yomie-go/data/id_ed25519 -port 21116 -relay-port 21117 -api-port 21114 -admin-port 21000 -log-format text -log-level info
Restart=always
RestartSec=5
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
SVC
systemctl daemon-reload

# Start
echo '[7/8] Starting Yomie Go...'
systemctl enable yomie-go
systemctl start yomie-go
sleep 3

# Verify
echo '[8/8] Verifying...'
if systemctl is-active --quiet yomie-go; then
    echo 'SUCCESS: Yomie Go is running!'
else
    echo 'FAILED: Service did not start.'
    journalctl -u yomie-go -n 30 --no-pager
    exit 1
fi
ss -tlnp | grep -E '2111[4-9]|21000' || true
ss -ulnp | grep 21116 || true
echo ''
echo '============================================'
echo '  ROLLBACK command:'
echo '  sudo systemctl stop yomie-go && sudo systemctl enable --now rustdesksignal rustdeskrelay'
echo '============================================'

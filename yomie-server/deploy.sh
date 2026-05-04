#!/usr/bin/env bash
# ============================================================================
# Yomie Go Server — Production Deployment Script
# Version: 1.0.0
# Replaces: hbbs + hbbr (Rust) with a single yomie-server binary (Go)
#
# Usage:
#   sudo ./deploy.sh                    # Interactive deployment
#   sudo ./deploy.sh --auto             # Automatic deployment with defaults
#   sudo ./deploy.sh --migrate-only     # Only migrate database, don't install
#   sudo ./deploy.sh --rollback         # Restore from backup and revert to Rust
#
# Requirements:
#   - Linux x86_64
#   - Root or sudo access
#   - Existing RustDesk/Yomie installation (optional, for migration)
# ============================================================================

set -euo pipefail

VERSION="1.0.0"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Default paths (matching yomie.sh conventions)
INSTALL_DIR="/opt/yomie"
OLD_INSTALL_DIR="/opt/rustdesk"
DATA_DIR="${INSTALL_DIR}/data"
BACKUP_DIR="${INSTALL_DIR}/backups"
LOG_DIR="/var/log/yomie"
SERVICE_NAME="yomie"

# Binary names
GO_BINARY="yomie-server-linux-amd64"
MIGRATE_BINARY="migrate-linux-amd64"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $*"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error() { echo -e "${RED}[ERROR]${NC} $*"; }
log_step()  { echo -e "${BLUE}[STEP]${NC} $*"; }

# ============================================================================
# Pre-flight checks
# ============================================================================

check_root() {
    if [[ $EUID -ne 0 ]]; then
        log_error "This script must be run as root (sudo)."
        exit 1
    fi
}

check_binary() {
    if [[ ! -f "${SCRIPT_DIR}/${GO_BINARY}" ]]; then
        log_error "Binary not found: ${SCRIPT_DIR}/${GO_BINARY}"
        log_error "Build it first: GOOS=linux GOARCH=amd64 go build -o ${GO_BINARY} ."
        exit 1
    fi
}

detect_existing_installation() {
    # Check for existing Rust hbbs/hbbr installation
    EXISTING_DB=""
    EXISTING_KEY=""
    EXISTING_DIR=""

    for dir in /opt/rustdesk /opt/yomie /opt/hbbs; do
        if [[ -d "$dir" ]]; then
            # Look for the database file
            for db in "$dir/db_v2.sqlite3" "$dir/data/db_v2.sqlite3" "$dir/db_v2.sqlite3"; do
                if [[ -f "$db" ]]; then
                    EXISTING_DB="$db"
                    EXISTING_DIR="$dir"
                    break 2
                fi
            done
        fi
    done

    # Find existing key file
    for dir in /opt/rustdesk /opt/yomie /opt/hbbs; do
        for key in "$dir/id_ed25519" "$dir/data/id_ed25519" "$dir/id_ed25519"; do
            if [[ -f "$key" ]]; then
                EXISTING_KEY="$key"
                break 2
            fi
        done
    done

    if [[ -n "$EXISTING_DB" ]]; then
        log_info "Found existing database: $EXISTING_DB"
    fi
    if [[ -n "$EXISTING_KEY" ]]; then
        log_info "Found existing key file: $EXISTING_KEY"
    fi
}

# ============================================================================
# Backup
# ============================================================================

create_backup() {
    local timestamp
    timestamp=$(date +%Y%m%d_%H%M%S)
    local backup_path="${BACKUP_DIR}/backup_${timestamp}"

    mkdir -p "$backup_path"

    log_step "Creating backup at ${backup_path}..."

    # Backup database
    if [[ -n "$EXISTING_DB" ]]; then
        cp -v "$EXISTING_DB" "${backup_path}/"
        # Also copy WAL/SHM files
        [[ -f "${EXISTING_DB}-wal" ]] && cp -v "${EXISTING_DB}-wal" "${backup_path}/"
        [[ -f "${EXISTING_DB}-shm" ]] && cp -v "${EXISTING_DB}-shm" "${backup_path}/"
        log_info "Database backed up"
    fi

    # Backup key file
    if [[ -n "$EXISTING_KEY" ]]; then
        cp -v "$EXISTING_KEY" "${backup_path}/"
        [[ -f "${EXISTING_KEY}.pub" ]] && cp -v "${EXISTING_KEY}.pub" "${backup_path}/"
        log_info "Key files backed up"
    fi

    # Backup existing services
    for svc in rustdesksignal rustdeskrelay yomie hbbs hbbr; do
        if [[ -f "/etc/systemd/system/${svc}.service" ]]; then
            cp -v "/etc/systemd/system/${svc}.service" "${backup_path}/"
        fi
    done

    # Backup existing binaries (for rollback)
    if [[ -n "$EXISTING_DIR" ]]; then
        for bin in hbbs hbbr yomie-server; do
            [[ -f "${EXISTING_DIR}/${bin}" ]] && cp -v "${EXISTING_DIR}/${bin}" "${backup_path}/"
        done
    fi

    echo "$timestamp" > "${backup_path}/backup.info"
    log_info "Backup complete: ${backup_path}"
    echo "$backup_path"
}

# ============================================================================
# Migration
# ============================================================================

migrate_database() {
    if [[ -z "$EXISTING_DB" ]]; then
        log_info "No existing database found — starting fresh."
        return 0
    fi

    local new_db="${DATA_DIR}/db_v2.sqlite3"

    log_step "Migrating database from Rust to Go schema..."

    if [[ -f "${SCRIPT_DIR}/tools/migrate/${MIGRATE_BINARY}" ]]; then
        "${SCRIPT_DIR}/tools/migrate/${MIGRATE_BINARY}" \
            -src "$EXISTING_DB" \
            -dst "$new_db"
    else
        log_warn "Migration tool not found. Copying database as-is."
        log_warn "The Go server will create new tables but won't read old 'peer' table data."
        cp "$EXISTING_DB" "$new_db"
    fi

    log_info "Database migration complete: $new_db"
}

# ============================================================================
# Installation
# ============================================================================

install_binary() {
    log_step "Installing Yomie Go server..."

    mkdir -p "$INSTALL_DIR" "$DATA_DIR" "$LOG_DIR" "$BACKUP_DIR"

    # Copy binary
    cp "${SCRIPT_DIR}/${GO_BINARY}" "${INSTALL_DIR}/yomie-server"
    chmod +x "${INSTALL_DIR}/yomie-server"

    # Copy migration tool
    if [[ -f "${SCRIPT_DIR}/tools/migrate/${MIGRATE_BINARY}" ]]; then
        cp "${SCRIPT_DIR}/tools/migrate/${MIGRATE_BINARY}" "${INSTALL_DIR}/migrate"
        chmod +x "${INSTALL_DIR}/migrate"
    fi

    # Copy existing key file if found
    if [[ -n "$EXISTING_KEY" ]]; then
        if [[ ! -f "${DATA_DIR}/id_ed25519" ]]; then
            cp "$EXISTING_KEY" "${DATA_DIR}/id_ed25519"
            [[ -f "${EXISTING_KEY}.pub" ]] && cp "${EXISTING_KEY}.pub" "${DATA_DIR}/id_ed25519.pub"
            log_info "Key file copied to ${DATA_DIR}/"
        fi
    fi

    log_info "Binary installed: ${INSTALL_DIR}/yomie-server"
}

stop_old_services() {
    log_step "Stopping old services..."

    for svc in rustdesksignal rustdeskrelay hbbs hbbr yomie; do
        if systemctl is-active --quiet "$svc" 2>/dev/null; then
            log_info "Stopping $svc..."
            systemctl stop "$svc" || true
        fi
        if systemctl is-enabled --quiet "$svc" 2>/dev/null; then
            log_info "Disabling $svc..."
            systemctl disable "$svc" || true
        fi
    done
}

create_systemd_service() {
    log_step "Creating systemd service..."

    cat > /etc/systemd/system/${SERVICE_NAME}.service << 'EOF'
[Unit]
Description=Yomie Server (Signal + Relay + API)
Documentation=https://github.com/shamstabraiz/Rustdesk-FreeConsole
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/yomie/data
ExecStart=/opt/yomie/yomie-server \
    -mode all \
    -db /opt/yomie/data/db_v2.sqlite3 \
    -key-file /opt/yomie/data/id_ed25519 \
    -port 21116 \
    -relay-port 21117 \
    -api-port 21114 \
    -admin-port 21000 \
    -log-format json \
    -audit-log /var/log/yomie/audit.jsonl
Restart=always
RestartSec=5
LimitNOFILE=65535

# Security hardening
ProtectSystem=strict
ReadWritePaths=/opt/yomie/data /var/log/yomie
NoNewPrivileges=true
ProtectHome=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true

# Environment variables (override defaults)
# Uncomment and set as needed:
#Environment=JWT_SECRET=your-secret-here
#Environment=ADMIN_PASSWORD=your-password-here
#Environment=FORCE_HTTPS=Y
#Environment=RELAY_MAX_CONNS_PER_IP=20
#Environment=TLS_CERT=/etc/letsencrypt/live/your-domain/fullchain.pem
#Environment=TLS_KEY=/etc/letsencrypt/live/your-domain/privkey.pem

[Install]
WantedBy=multi-user.target
EOF

    systemctl daemon-reload
    log_info "Systemd service created: ${SERVICE_NAME}"
}

enable_and_start() {
    log_step "Enabling and starting Yomie server..."

    systemctl enable "${SERVICE_NAME}"
    systemctl start "${SERVICE_NAME}"

    sleep 2

    if systemctl is-active --quiet "${SERVICE_NAME}"; then
        log_info "Yomie server is running!"
    else
        log_error "Service failed to start. Check: journalctl -u ${SERVICE_NAME} -n 50"
        exit 1
    fi
}

# ============================================================================
# Rollback
# ============================================================================

rollback() {
    log_step "Rolling back to previous installation..."

    # Find latest backup
    local latest_backup
    latest_backup=$(ls -1td "${BACKUP_DIR}"/backup_* 2>/dev/null | head -1)

    if [[ -z "$latest_backup" ]]; then
        log_error "No backups found in ${BACKUP_DIR}"
        exit 1
    fi

    log_info "Restoring from: $latest_backup"

    # Stop new service
    systemctl stop "${SERVICE_NAME}" 2>/dev/null || true
    systemctl disable "${SERVICE_NAME}" 2>/dev/null || true

    # Restore old services
    for svc_file in "${latest_backup}"/*.service; do
        [[ -f "$svc_file" ]] && cp "$svc_file" /etc/systemd/system/
    done

    # Restore database
    for db_file in "${latest_backup}"/db_v2.sqlite3*; do
        [[ -f "$db_file" ]] && cp "$db_file" "${OLD_INSTALL_DIR}/"
    done

    # Restore key files
    for key_file in "${latest_backup}"/id_ed25519*; do
        [[ -f "$key_file" ]] && cp "$key_file" "${OLD_INSTALL_DIR}/"
    done

    # Restore old binaries
    for bin in hbbs hbbr; do
        [[ -f "${latest_backup}/${bin}" ]] && cp "${latest_backup}/${bin}" "${OLD_INSTALL_DIR}/"
    done

    systemctl daemon-reload

    # Re-enable old services
    for svc in rustdesksignal rustdeskrelay; do
        if [[ -f "/etc/systemd/system/${svc}.service" ]]; then
            systemctl enable "$svc" 2>/dev/null || true
            systemctl start "$svc" 2>/dev/null || true
        fi
    done

    log_info "Rollback complete. Old services should be running."
}

# ============================================================================
# Status / Verification
# ============================================================================

verify_deployment() {
    echo ""
    log_step "Verifying deployment..."
    echo ""

    # Check service
    if systemctl is-active --quiet "${SERVICE_NAME}"; then
        log_info "Service: RUNNING"
    else
        log_error "Service: NOT RUNNING"
    fi

    # Check API health
    sleep 1
    if curl -sf http://localhost:21114/api/health > /dev/null 2>&1; then
        local health
        health=$(curl -sf http://localhost:21114/api/health)
        log_info "API Health: ${health}"
    else
        log_warn "API not responding yet (may still be starting)"
    fi

    # Check ports
    echo ""
    log_info "Listening ports:"
    ss -tlnp | grep -E '2111[4-9]|21000' || true

    # Show initial credentials from journal
    echo ""
    log_info "Check initial admin credentials:"
    echo "  journalctl -u ${SERVICE_NAME} | grep 'INITIAL ADMIN'"
    echo ""

    # Show useful commands
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  USEFUL COMMANDS"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Status:     systemctl status ${SERVICE_NAME}"
    echo "  Logs:       journalctl -u ${SERVICE_NAME} -f"
    echo "  Stop:       systemctl stop ${SERVICE_NAME}"
    echo "  Start:      systemctl start ${SERVICE_NAME}"
    echo "  Restart:    systemctl restart ${SERVICE_NAME}"
    echo "  Rollback:   $0 --rollback"
    echo "  Health:     curl http://localhost:21114/api/health"
    echo "  Stats:      curl http://localhost:21114/api/server/stats"
    echo "  Admin TCP:  nc localhost 21000"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
}

# ============================================================================
# Firewall
# ============================================================================

configure_firewall() {
    log_step "Configuring firewall rules..."

    if command -v ufw &>/dev/null; then
        ufw allow 21114/tcp comment "Yomie API"
        ufw allow 21115/tcp comment "Yomie NAT Test"
        ufw allow 21116/tcp comment "Yomie Signal TCP"
        ufw allow 21116/udp comment "Yomie Signal UDP"
        ufw allow 21117/tcp comment "Yomie Relay"
        ufw allow 21118/tcp comment "Yomie WS Signal"
        ufw allow 21119/tcp comment "Yomie WS Relay"
        log_info "UFW rules added"
    elif command -v firewall-cmd &>/dev/null; then
        for port in 21114/tcp 21115/tcp 21116/tcp 21116/udp 21117/tcp 21118/tcp 21119/tcp; do
            firewall-cmd --permanent --add-port="$port" 2>/dev/null || true
        done
        firewall-cmd --reload 2>/dev/null || true
        log_info "firewalld rules added"
    else
        log_warn "No firewall manager found. Ensure ports 21114-21119 are open."
    fi
}

# ============================================================================
# Main
# ============================================================================

print_banner() {
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  Yomie Go Server — Deployment v${VERSION}"
    echo "  Single binary replacing hbbs + hbbr"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
}

main() {
    print_banner
    check_root

    # Parse arguments
    AUTO=false
    MIGRATE_ONLY=false
    DO_ROLLBACK=false

    for arg in "$@"; do
        case "$arg" in
            --auto)         AUTO=true ;;
            --migrate-only) MIGRATE_ONLY=true ;;
            --rollback)     DO_ROLLBACK=true ;;
            -h|--help)
                echo "Usage: $0 [--auto] [--migrate-only] [--rollback]"
                exit 0
                ;;
        esac
    done

    if $DO_ROLLBACK; then
        detect_existing_installation
        rollback
        exit 0
    fi

    check_binary
    detect_existing_installation

    if ! $AUTO; then
        echo "This script will:"
        echo "  1. Create a backup of the existing database and keys"
        echo "  2. Stop old hbbs/hbbr services"
        echo "  3. Migrate the database to the new Go schema"
        echo "  4. Install the Yomie Go server"
        echo "  5. Create and start a systemd service"
        echo "  6. Configure firewall rules"
        echo ""
        if [[ -n "$EXISTING_DB" ]]; then
            echo "  Existing DB: $EXISTING_DB"
        else
            echo "  No existing database found — fresh installation."
        fi
        echo ""
        read -rp "Continue? [Y/n] " confirm
        if [[ "${confirm,,}" == "n" ]]; then
            echo "Aborted."
            exit 0
        fi
    fi

    # Step 1: Backup
    mkdir -p "$BACKUP_DIR"
    if [[ -n "$EXISTING_DB" ]] || [[ -n "$EXISTING_KEY" ]]; then
        create_backup
    fi

    if $MIGRATE_ONLY; then
        mkdir -p "$DATA_DIR"
        migrate_database
        log_info "Migration-only mode — done."
        exit 0
    fi

    # Step 2: Stop old services
    stop_old_services

    # Step 3: Install binary
    install_binary

    # Step 4: Migrate database
    migrate_database

    # Step 5: Create systemd service
    create_systemd_service

    # Step 6: Firewall
    configure_firewall

    # Step 7: Start
    enable_and_start

    # Step 8: Verify
    verify_deployment
}

main "$@"

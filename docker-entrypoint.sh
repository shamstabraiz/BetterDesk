#!/bin/sh
# Docker Entrypoint for Yomie Console (Node.js)
# ---------------------------------------------------
# The Node.js application handles all database initialization,
# migration, and admin user creation automatically on startup.
# This script only provides logging and environment validation.
set -e

echo "========================================"
echo "  Yomie Console - Container Startup"
echo "  Version: 2.4.0 (Node.js)"
echo "========================================"

# Log configuration
echo ""
echo "Configuration:"
echo "  NODE_ENV:        ${NODE_ENV:-production}"
echo "  PORT:            ${PORT:-5000}"
echo "  SERVER_BACKEND:  ${SERVER_BACKEND:-yomie}"
echo "  DB_TYPE:         ${DB_TYPE:-sqlite}"
echo "  RUSTDESK_PATH:   ${RUSTDESK_PATH:-/opt/rustdesk}"
echo "  DATA_DIR:        ${DATA_DIR:-/app/data}"
echo ""

# Ensure data directories exist
mkdir -p "${DATA_DIR:-/app/data}" 2>/dev/null || true

# Docker quick-start fix: if DB_PATH is on a read-only volume, relocate to DATA_DIR
DB_FILE="${DB_PATH:-/opt/rustdesk/db_v2.sqlite3}"
DB_DIR_CHECK="$(dirname "$DB_FILE")"
if [ ! -w "$DB_DIR_CHECK" ] && [ -n "${DATA_DIR}" ]; then
    NEW_DB="${DATA_DIR}/db_v2.sqlite3"
    echo "  DB path $DB_FILE is read-only, relocating to $NEW_DB"
    # Copy existing DB from read-only mount if it exists and we don't have one yet
    if [ -f "$DB_FILE" ] && [ ! -f "$NEW_DB" ]; then
        cp "$DB_FILE" "$NEW_DB" 2>/dev/null || true
    fi
    export DB_PATH="$NEW_DB"
    DB_FILE="$NEW_DB"
fi

# Verify SQLite database path is writable (catches :ro volume mounts early)
DB_DIR="$(dirname "$DB_FILE")"
if [ "${DB_TYPE:-sqlite}" = "sqlite" ]; then
    if [ -f "$DB_FILE" ] && [ ! -w "$DB_FILE" ]; then
        echo ""
        echo "ERROR: Database file $DB_FILE is not writable!"
        echo "  This usually means the volume is mounted read-only (:ro)."
        echo "  Fix: In docker-compose.yml, change the console volume mount from:"
        echo "    rustdesk-data:/opt/rustdesk:ro"
        echo "  to:"
        echo "    rustdesk-data:/opt/rustdesk"
        echo ""
        echo "  Then run: docker compose down && docker compose up -d"
        echo ""
        exit 1
    fi
    if [ ! -w "$DB_DIR" ]; then
        echo ""
        echo "WARNING: Database directory $DB_DIR is not writable."
        echo "  SQLite needs write access for WAL journal files (.db-wal, .db-shm)."
        echo "  Check volume mount permissions in docker-compose.yml."
        echo ""
    fi
fi

# Wait for Yomie server (hbbs) to be available if using yomie backend
if [ "${SERVER_BACKEND}" = "yomie" ] && [ -n "${HBBS_API_URL}" ]; then
    echo "Waiting for Yomie server..."
    RETRIES=0
    MAX_RETRIES=30
    while [ "$RETRIES" -lt "$MAX_RETRIES" ]; do
        if curl -sf "${HBBS_API_URL}/health" >/dev/null 2>&1 || \
           curl -sf "${BETTERDESK_API_URL:-${HBBS_API_URL}}" >/dev/null 2>&1; then
            echo "  Yomie server is ready"
            break
        fi
        RETRIES=$((RETRIES + 1))
        echo "  Waiting for server... ($RETRIES/$MAX_RETRIES)"
        sleep 2
    done
    if [ "$RETRIES" -ge "$MAX_RETRIES" ]; then
        echo "  WARNING: Yomie server not reachable after ${MAX_RETRIES} attempts"
        echo "  Starting console anyway (some features may be unavailable)..."
    fi
fi

# Wait for PostgreSQL if configured
if [ "${DB_TYPE}" = "postgresql" ] && [ -n "${DATABASE_URL}" ]; then
    echo "Waiting for PostgreSQL..."
    RETRIES=0
    MAX_RETRIES=30
    # Extract host:port from DATABASE_URL (postgres://user:pass@host:port/db)
    PG_HOST=$(echo "$DATABASE_URL" | sed -n 's|.*@\([^:/]*\).*|\1|p')
    PG_PORT=$(echo "$DATABASE_URL" | sed -n 's|.*:\([0-9]*\)/.*|\1|p')
    PG_PORT=${PG_PORT:-5432}
    while [ "$RETRIES" -lt "$MAX_RETRIES" ]; do
        if nc -z "$PG_HOST" "$PG_PORT" 2>/dev/null; then
            echo "  PostgreSQL is ready ($PG_HOST:$PG_PORT)"
            break
        fi
        RETRIES=$((RETRIES + 1))
        echo "  Waiting for PostgreSQL... ($RETRIES/$MAX_RETRIES)"
        sleep 2
    done
    if [ "$RETRIES" -ge "$MAX_RETRIES" ]; then
        echo "  WARNING: PostgreSQL not reachable after ${MAX_RETRIES} attempts"
    fi
fi

echo ""
echo "Starting Yomie Console..."
echo "  Web Interface: http://localhost:${PORT:-5000}"
echo "  Client API:    port ${API_PORT:-21121}"
echo ""

# Start Node.js application (NODE_WATCH=1 enables node --watch for local dev)
if [ -n "${NODE_WATCH}" ]; then
    echo "  Dev: file watching enabled (NODE_WATCH)"
    exec node --watch server.js
fi
exec node server.js
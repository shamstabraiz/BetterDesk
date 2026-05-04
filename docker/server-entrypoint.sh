#!/bin/sh
# Yomie Server — Docker Entrypoint
# Fixes volume file permissions before dropping to non-root user
set -e

DATA_DIR="/opt/rustdesk"

# Fix ownership of volume-mounted data directory.
# Docker volumes preserve UID/GID from the host or previous container,
# which may not match the yomie user (10001) in this container.
# This is especially important for id_ed25519 (mode 600) — if owned by
# a different UID, the server cannot read the private key.
if [ "$(id -u)" = "0" ]; then
    chown -R yomie:yomie "$DATA_DIR" 2>/dev/null || true
    # Ensure private key is readable by yomie
    if [ -f "$DATA_DIR/id_ed25519" ]; then
        chmod 600 "$DATA_DIR/id_ed25519"
        chown yomie:yomie "$DATA_DIR/id_ed25519"
    fi
    # Drop privileges and re-exec
    exec su-exec yomie "$@"
else
    exec "$@"
fi

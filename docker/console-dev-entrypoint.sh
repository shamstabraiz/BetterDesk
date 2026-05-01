#!/bin/sh
# Dev console: bind-mounted /app, separate node_modules volume, then shared startup logic
set -e

cd /app
export NODE_ENV=development
export NODE_WATCH=1

mkdir -p "${DATA_DIR:-/app/data}"

if [ ! -d /app/node_modules/bcrypt ]; then
    echo "Dev: installing npm dependencies (first run or empty volume)..."
    npm install
fi

exec /etc/betterdesk/docker-entrypoint.sh

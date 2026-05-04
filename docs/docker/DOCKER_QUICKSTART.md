# 🚀 Yomie Docker Quick Start

Get Yomie running in **30 seconds** with pre-built images from GitHub Container Registry.

## Prerequisites

- Docker 20.10+
- docker-compose v2.0+ (or `docker compose` plugin)
- Open ports: 21114-21119, 5000

## 🏃 Quick Start (3 Commands)

```bash
# 1. Download docker-compose file
curl -fsSL https://raw.githubusercontent.com/shamstabraiz/Rustdesk-FreeConsole/main/docker-compose.quick.yml -o docker-compose.yml

# 2. Start Yomie
docker compose up -d

# 3. Get admin password
docker compose logs console 2>&1 | grep -i "Admin password"
```

**Done!** Open http://localhost:5000 and log in with `admin` / (password from step 3).

---

## 📦 What Gets Installed

| Service | Port | Description |
|---------|------|-------------|
| Server | 21116 TCP/UDP | Signal server (device registration) |
| Server | 21117 | Relay server (connections) |
| Server | 21114 | HTTP API |
| Console | 5000 | Web management panel |
| Console | 21121 | RustDesk client API |

## 🔧 Configuration

### Custom Admin Password

```bash
# Set before first start
ADMIN_PASSWORD=YourSecurePass123 docker compose up -d
```

### PostgreSQL Instead of SQLite

```yaml
# Add to docker-compose.yml
services:
  postgres:
    image: postgres:15-alpine
    environment:
      POSTGRES_DB: yomie
      POSTGRES_USER: yomie
      POSTGRES_PASSWORD: secretpassword
    volumes:
      - postgres-data:/var/lib/postgresql/data

  server:
    environment:
      - DB_URL=postgres://yomie:secretpassword@postgres:5432/yomie
    depends_on:
      - postgres

volumes:
  postgres-data:
```

### SSL/TLS

See [HTTPS_SETUP.md](../setup/HTTPS_SETUP.md) for full instructions.

Quick self-signed cert:
```bash
mkdir -p certs
openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
  -keyout certs/key.pem -out certs/cert.pem \
  -subj "/CN=yomie.local"
```

---

## 🔄 Updates

```bash
docker compose pull
docker compose up -d
```

## 🗑️ Uninstall

```bash
docker compose down -v  # -v removes volumes (data)
```

## 📊 Check Status

```bash
# All services running?
docker compose ps

# Server logs
docker compose logs server

# Console logs
docker compose logs console

# Health check
curl http://localhost:21114/api/health
```

---

## ❓ Troubleshooting

### "denied" or "pull access denied" when starting

This means the pre-built images are not yet published to GitHub Container Registry.

**Solution A — Build locally (recommended):**
```bash
# Use the full docker-compose.yml which builds images from source
git clone https://github.com/shamstabraiz/Rustdesk-FreeConsole.git
cd Rustdesk-FreeConsole
docker compose -f docker-compose.yml up -d --build
```

**Solution B — Wait for images to be published:**

The repository maintainer needs to trigger the Docker publish workflow:
1. Go to: GitHub repo → Actions → "Build & Publish Docker Images"
2. Click "Run workflow" → Branch: main → Click "Run workflow"
3. Wait ~10 minutes for images to build
4. Once images are published, retry `docker compose up -d`

**Solution C — Authenticate (if repo is private):**
```bash
# Create a GitHub Personal Access Token with 'read:packages' scope
docker login ghcr.io -u YOUR_GITHUB_USERNAME -p YOUR_GITHUB_TOKEN
docker compose up -d
```

### "Cannot connect to devices"

1. Check firewall allows ports 21116-21117
2. Verify server is healthy: `curl http://localhost:21114/api/health`
3. Check relay server: `docker compose logs server | grep relay`

### "Web console shows 0 devices"

1. Verify API key sync: `docker compose exec console cat /opt/rustdesk/.api_key`
2. Restart console: `docker compose restart console`

### "Connection refused on port 21116"

1. Wait 30 seconds for server to start
2. Check server health: `docker compose ps`
3. View server logs: `docker compose logs server`

### Need more help?

See [DOCKER_TROUBLESHOOTING.md](../docker/DOCKER_TROUBLESHOOTING.md) for advanced issues.

---

## 🏗️ Build from Source (Advanced)

If you need custom modifications:

```bash
git clone https://github.com/shamstabraiz/Rustdesk-FreeConsole.git
cd Rustdesk-FreeConsole
docker compose -f docker-compose.yml up -d --build
```

---

## 📝 RustDesk Client Configuration

Configure your RustDesk clients with:

| Setting | Value |
|---------|-------|
| ID Server | `YOUR_SERVER_IP:21116` |
| Relay Server | `YOUR_SERVER_IP:21117` |
| API Server | `http://YOUR_SERVER_IP:21114` |
| Key | (get from web console Settings page) |

Or scan the QR code from the web console Settings page.

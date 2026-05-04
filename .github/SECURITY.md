# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 3.0.x   | :white_check_mark: |
| 2.4.x   | :white_check_mark: |
| < 2.4   | :x:                |

## Reporting a Vulnerability

We take the security of Yomie seriously. If you discover a security vulnerability, please report it responsibly.

### How to Report

1. **Do NOT open a public GitHub Issue** for security vulnerabilities.
2. Email your findings to **security@shamstabraiz.com** with:
   - Description of the vulnerability
   - Steps to reproduce
   - Affected component (Go server, Node.js console, desktop client, installer scripts)
   - Potential impact assessment
   - Any suggested fix (optional but appreciated)

### What to Expect

| Timeline | Action |
|----------|--------|
| **24 hours** | Acknowledgment of your report |
| **72 hours** | Initial assessment and severity classification |
| **7 days** | Fix development begins for Critical/High issues |
| **30 days** | Patch released (or interim mitigation communicated) |
| **90 days** | Public disclosure (coordinated with reporter) |

### Severity Classification

| Severity | Examples |
|----------|----------|
| **Critical** | Remote code execution, authentication bypass, SQL injection, private key exposure |
| **High** | Privilege escalation, CSRF on sensitive actions, session fixation, brute-force without rate limiting |
| **Medium** | Information disclosure, XSS, insecure defaults, missing input validation |
| **Low** | Verbose error messages, minor information leakage, missing security headers |

### Scope

The following components are in scope:

- **Yomie Go Server** (`yomie-server/`) — signal, relay, API, database
- **Node.js Web Console** (`web-nodejs/`) — Express.js app, routes, middleware
- **Desktop Client** (`yomie-client/`) — Tauri app, Rust backend, TypeScript frontend
- **CDAP Agent** (`yomie-agent/`) — Go agent binary
- **Installer Scripts** (`yomie.sh`, `yomie.ps1`, `yomie-docker.sh`)
- **Docker Images** (`Dockerfile*`, `docker-compose*.yml`)
- **SDKs** (`sdks/python/`, `sdks/nodejs/`)

### Out of Scope

- Third-party dependencies (report to upstream maintainers, but notify us if it affects Yomie)
- Social engineering attacks
- Denial of service via network flooding (volumetric attacks)
- Issues in archived components (`archive/`)

### Recognition

We gratefully acknowledge security researchers who report vulnerabilities responsibly:

- Your name (or alias) will be added to our Security Hall of Fame (with your permission)
- We will credit you in the relevant release notes

### Security Best Practices for Deployers

1. Always use TLS certificates for production deployments (`--tls-signal`, `--tls-relay`)
2. Keep the web console bound to `127.0.0.1` or behind a reverse proxy
3. Use PostgreSQL (not SQLite) for multi-user production environments
4. Rotate API keys regularly
5. Enable TOTP 2FA for all admin accounts
6. Review audit logs periodically
7. Keep Yomie updated to the latest supported version

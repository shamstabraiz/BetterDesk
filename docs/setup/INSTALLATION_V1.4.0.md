# 🚀 BetterDesk Console v1.4.0 - Installation & Update Guide

## 📋 What's New in v1.4.0

### 🔐 Authentication System
- **User login with username/password**
- **Session management** (24-hour sessions)
- **Password hashing** with bcrypt
- **Default admin account** created automatically

### 👥 Role-Based Access Control
- **Admin**: Full access (manage users, devices, settings)
- **Operator**: Can ban/unban, edit devices, view audit log
- **Viewer**: Read-only access

### 🎨 New UI Features
- **Sidebar navigation menu** with glassmorphism design
- **Responsive design** for mobile/tablet
- **User profile display** in sidebar
- **Better page organization**

### 🔒 Security Improvements
- **All API endpoints protected** with authentication
- **Audit logging** for all operations
- **XSS protection** with MarkupSafe
- **Session expiry** and automatic cleanup
- **CSRF protection ready** (can be enabled)

---

## 🆕 New Installation

### Quick Install (Recommended)

```bash
# Download and run installer
curl -O https://raw.githubusercontent.com/shamstabraiz/Rustdesk-FreeConsole/main/install-improved.sh
chmod +x install-improved.sh
sudo ./install-improved.sh
```

The installer will:
1. Install RustDesk HBBS/HBBR servers (if not present)
2. Install web console with authentication
3. Create database with auth tables
4. Generate default admin credentials
5. Configure systemd services

---

## 🔄 Updating from v1.3.0 or older

### Automatic Update (Recommended)

```bash
# Download update script
cd /path/to/Rustdesk-FreeConsole
chmod +x update-to-v1.4.0.sh
sudo ./update-to-v1.4.0.sh
```

### What the Update Script Does:

1. **Detects current version** automatically
2. **Creates backup** of:
   - Web console files
   - Database
   - Configuration
3. **Installs new dependencies**:
   - `bcrypt` (password hashing)
   - `markupsafe` (XSS protection)
4. **Updates web files**:
   - New `auth.py` module
   - Updated `app.py` with auth endpoints
   - New `login.html` template
   - Updated `index.html` with sidebar
   - New CSS and JavaScript files
5. **Runs database migration**:
   - Adds `users` table
   - Adds `sessions` table
   - Adds `audit_log` table
   - Creates default admin user
6. **Restarts services**
7. **Shows admin credentials** (if new)

### Manual Update Steps

If you prefer manual update:

```bash
# 1. Backup
sudo cp -r /opt/BetterDeskConsole /opt/BetterDeskConsole.backup
sudo cp /opt/rustdesk/db_v2.sqlite3 /opt/rustdesk/db_v2.sqlite3.backup

# 2. Install dependencies
sudo pip3 install bcrypt markupsafe --break-system-packages

# 3. Copy new files
cd /path/to/Rustdesk-FreeConsole
sudo cp web/auth.py /opt/BetterDeskConsole/
sudo cp web/app.py /opt/BetterDeskConsole/
sudo cp web/templates/login.html /opt/BetterDeskConsole/templates/
sudo cp web/templates/index.html /opt/BetterDeskConsole/templates/
sudo cp web/static/style.css /opt/BetterDeskConsole/static/
sudo cp web/static/script.js /opt/BetterDeskConsole/static/

# 4. Run migration
sudo python3 migrations/v1.4.0_auth_system.py

# 5. Restart service
sudo systemctl restart betterdesk
```

---

## 🔑 First Login

After installation/update, you'll receive default admin credentials:

```
========================================
DEFAULT ADMIN CREDENTIALS:
========================================
Username: admin
Password: <randomly-generated-password>
========================================
```

**⚠️ IMPORTANT:**
1. **Save these credentials** in a secure location
2. **Login immediately** and change the password
3. **Delete credentials file**: `sudo rm /opt/BetterDeskConsole/admin_credentials.txt`

### Accessing the Console

```bash
# Local access (on server)
http://localhost:5000

# Remote access (via SSH tunnel - RECOMMENDED)
ssh -L 8080:localhost:5000 user@your-server
# Then open: http://localhost:8080
```

---

## 👥 User Management

### Creating Additional Users (Admin Only)

You can create additional users via Python console:

```python
cd /opt/BetterDeskConsole/web
python3 -c "
from auth import create_user, ROLE_ADMIN, ROLE_OPERATOR, ROLE_VIEWER

# Create operator
create_user('operator1', 'SecurePassword123', ROLE_OPERATOR)

# Create viewer
create_user('viewer1', 'SecurePassword123', ROLE_VIEWER)

print('Users created successfully')
"
```

### User Roles Explained

| Role | Permissions |
|------|-------------|
| **Admin** | Full access: manage users, devices, settings, view audit log |
| **Operator** | Ban/unban devices, edit device info, view audit log |
| **Viewer** | Read-only: view devices and statistics |

---

## 🔒 Security Best Practices

### 1. Change Default Password

```
1. Login with default credentials
2. Click your name in sidebar
3. Go to Settings
4. Click "Change Password"
5. Enter current password and new password
```

### 2. Use SSH Tunnel (Recommended)

**Never expose port 5000 to the internet!**

```bash
# From your local machine
ssh -L 8080:localhost:5000 user@your-server

# Keep this terminal open
# Access console at: http://localhost:8080
```

### 3. Firewall Configuration

```bash
# Block external access to console
sudo ufw deny 5000

# Allow only SSH
sudo ufw allow 22

# Allow RustDesk ports
sudo ufw allow 21115:21117/tcp
sudo ufw allow 21116/udp
```

### 4. Regular Backups

```bash
# Backup database
sudo cp /opt/rustdesk/db_v2.sqlite3 /backup/db_v2.sqlite3.$(date +%Y%m%d)

# Backup web console
sudo tar -czf /backup/betterdesk-$(date +%Y%m%d).tar.gz /opt/BetterDeskConsole
```

---

## 🐛 Troubleshooting

### Login Issues

**Problem:** "Invalid username or password"

**Solutions:**
1. Check credentials file: `sudo cat /opt/BetterDeskConsole/admin_credentials.txt`
2. Reset admin password:
   ```python
   cd /opt/BetterDeskConsole/web
   python3 -c "
   from auth import reset_password
   reset_password(1, 'NewPassword123')  # User ID 1 is admin
   print('Password reset successfully')
   "
   ```

### Session Expired

**Problem:** Automatically logged out

**Solution:** Sessions expire after 24 hours. This is normal security behavior.

### Migration Errors

**Problem:** Database migration fails

**Solutions:**
1. Check database permissions:
   ```bash
   ls -la /opt/rustdesk/db_v2.sqlite3
   sudo chown root:root /opt/rustdesk/db_v2.sqlite3
   ```

2. Restore from backup if needed:
   ```bash
   sudo cp /opt/rustdesk/db_v2.sqlite3.backup-pre-v1.4.0 /opt/rustdesk/db_v2.sqlite3
   ```

3. Try migration again:
   ```bash
   sudo python3 migrations/v1.4.0_auth_system.py
   ```

### Service Won't Start

**Check logs:**
```bash
sudo journalctl -u betterdesk -n 50 --no-pager
```

**Common issues:**
- Missing dependencies: `sudo pip3 install bcrypt markupsafe --break-system-packages`
- Database locked: `sudo systemctl restart rustdesksignal`
- Port conflict: `sudo netstat -tulpn | grep 5000`

---

## 📊 Audit Log

View user actions:

```bash
# View audit log in database
sqlite3 /opt/rustdesk/db_v2.sqlite3 "
SELECT 
    u.username,
    a.action,
    a.device_id,
    a.timestamp,
    a.ip_address
FROM audit_log a
LEFT JOIN users u ON a.user_id = u.id
ORDER BY a.timestamp DESC
LIMIT 50;
"
```

---

## 🔄 Rollback to v1.3.0

If you need to rollback:

```bash
# Stop service
sudo systemctl stop betterdesk

# Restore backup
sudo rm -rf /opt/BetterDeskConsole
sudo cp -r /opt/BetterDeskConsole.backup /opt/BetterDeskConsole

# Restore database
sudo cp /opt/rustdesk/db_v2.sqlite3.backup /opt/rustdesk/db_v2.sqlite3

# Restart service
sudo systemctl start betterdesk
```

---

## 📞 Support

- **GitHub Issues**: https://github.com/shamstabraiz/Rustdesk-FreeConsole/issues
- **Documentation**: See `docs/` folder
- **Security Issues**: Use GitHub Security Advisories (private reporting)

---

## ✅ Post-Installation Checklist

- [ ] Login with default credentials works
- [ ] Changed admin password
- [ ] Deleted credentials file
- [ ] Configured firewall (block port 5000 externally)
- [ ] Set up SSH tunnel for remote access
- [ ] Tested device list loading
- [ ] Tested ban/unban functionality
- [ ] Created backup of database
- [ ] Verified audit logging works

---

*Last updated: 14 stycznia 2026*  
*Version: 1.4.0*

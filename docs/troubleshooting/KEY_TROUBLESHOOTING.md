# 🔑 RustDesk Key Troubleshooting Guide

## 📋 Table of Contents
- [Common Symptoms](#common-symptoms)
- [Understanding RustDesk Keys](#understanding-rustdesk-keys)
- [Diagnosis Steps](#diagnosis-steps)
- [Solutions](#solutions)
- [Prevention](#prevention)
- [Emergency Recovery](#emergency-recovery)

---

## 🚨 Common Symptoms

If you experience any of these issues, you likely have a key mismatch problem:

### "The keys do not match"
- Clients cannot connect to server
- Error message appears immediately on connection attempt
- Previously working connections suddenly fail

### "Remote desktop is offline"
- Intermittent connectivity issues
- Some devices work, others don't
- Connection was working before BetterDesk installation

### Public Key Mismatch in WebConsole
- Key displayed in BetterDesk console doesn't match your records
- Multiple `.pub` files exist in RustDesk directory
- Key file has unexpected name

---

## 🔐 Understanding RustDesk Keys

### What are these keys?

RustDesk uses **ED25519 cryptographic keys** for authentication:

- **Private Key**: `id_ed25519` (never share this!)
  - Stays on the server
  - Used to prove server identity
  - Should have `600` permissions (owner read/write only)

- **Public Key**: `id_ed25519.pub` (distribute to clients)
  - Configured in each RustDesk client
  - Used to verify server identity
  - Should have `644` permissions (readable by all)

### Why do keys matter?

```
┌─────────────┐                    ┌─────────────┐
│   Client    │                    │   Server    │
│             │                    │             │
│ Has: Public │◄──── Validates ───►│ Has: Private│
│      Key    │      Connection    │      Key    │
└─────────────┘                    └─────────────┘

If keys don't match = Connection REJECTED
```

**CRITICAL**: If the public key in a client doesn't match the server's private key, connection will fail.

---

## 🔍 Diagnosis Steps

### Step 1: Check Existing Keys

```bash
# List all key files
ls -lah /opt/rustdesk/*.pub
ls -lah /opt/rustdesk/id_ed25519
```

**Expected output:**
```
-rw-------  1 root root  411 Jan 13 10:30 id_ed25519
-rw-r--r--  1 root root  103 Jan 13 10:30 id_ed25519.pub
```

### Step 2: Display Current Public Key

```bash
cat /opt/rustdesk/id_ed25519.pub
```

**Compare this with:**
- What's shown in BetterDesk WebConsole
- What's configured in your RustDesk clients

### Step 3: Check for Backups

```bash
# Check for backup directories
ls -d /opt/rustdesk-backup-*

# Check for key backups
ls -lah /opt/rustdesk/*.backup*
```

### Step 4: Verify Services

```bash
# Check if services are running
systemctl status rustdesksignal.service
systemctl status rustdeskrelay.service

# Check logs for key errors
journalctl -u rustdesksignal -n 50 --no-pager | grep -i "key\|error"
```

---

## 💡 Solutions

### Solution 1: Keys Were Accidentally Changed

**Scenario**: BetterDesk installation regenerated your keys

**Fix**: Restore from backup

```bash
# Find your backup
ls -d /opt/rustdesk-backup-*

# Most recent backup
BACKUP=$(ls -d /opt/rustdesk-backup-* | sort | tail -1)

# Stop services
sudo systemctl stop rustdesksignal rustdeskrelay

# Restore keys
sudo cp $BACKUP/id_ed25519* /opt/rustdesk/

# Fix permissions
sudo chmod 600 /opt/rustdesk/id_ed25519
sudo chmod 644 /opt/rustdesk/id_ed25519.pub

# Restart services
sudo systemctl start rustdesksignal rustdeskrelay

# Verify
cat /opt/rustdesk/id_ed25519.pub
```

### Solution 2: Fix Key Permissions Manually

**Easiest method** - fix permissions directly:

```bash
# Set correct permissions for encryption keys
sudo chmod 600 /opt/rustdesk/id_ed25519
sudo chmod 644 /opt/rustdesk/id_ed25519.pub

# Verify ownership
sudo chown root:root /opt/rustdesk/id_ed25519*

# Restart services
sudo systemctl restart rustdesksignal rustdeskrelay betterdesk
```

**Verify it works:**
```bash
cat /opt/rustdesk/id_ed25519.pub
```

### Solution 3: Multiple .pub Files Exist

**Scenario**: Directory contains multiple `.pub` files with different names

**Diagnosis:**
```bash
# Find all .pub files
find /opt/rustdesk -name "*.pub"
```

**Fix:**

**Option A**: Remove incorrect files (if you know which is wrong)
```bash
# Backup first!
sudo cp -r /opt/rustdesk /opt/rustdesk-backup-manual

# Remove incorrect file
sudo rm /opt/rustdesk/wrong_key.pub
```

**Option B**: Identify correct key
```bash
# Check HBBS logs to see which key it's using
sudo journalctl -u rustdesksignal | grep -i "public key\|key loaded"

# Or check which key was created by HBBS
stat /opt/rustdesk/*.pub
```

### Solution 4: Keys Are Corrupted

**Symptoms:**
- Keys exist but don't work
- Strange characters in key file
- File sizes are wrong

**Fix - Last Resort** (regenerate keys):

```bash
# STOP! Make backup first!
sudo cp -r /opt/rustdesk /opt/rustdesk-backup-emergency

# Remove corrupted keys
sudo rm -f /opt/rustdesk/id_ed25519*

# Generate new keypair
sudo ssh-keygen -t ed25519 -f /opt/rustdesk/id_ed25519 -N ""

# Fix permissions
sudo chmod 600 /opt/rustdesk/id_ed25519
sudo chmod 644 /opt/rustdesk/id_ed25519.pub

# Restart services
sudo systemctl restart rustdesksignal rustdeskrelay

# Display new public key
echo "NEW PUBLIC KEY - CONFIGURE THIS IN ALL CLIENTS:"
cat /opt/rustdesk/id_ed25519.pub
```

⚠️ **After regenerating keys, you MUST reconfigure ALL RustDesk clients!**

---

## 🛡️ Prevention

### Best Practices

1. **Always Backup Before Installing BetterDesk**
   ```bash
   sudo cp -r /opt/rustdesk /opt/rustdesk-backup-$(date +%Y%m%d)
   ```

2. **Save Your Public Key Externally**
   ```bash
   cat /opt/rustdesk/id_ed25519.pub > ~/rustdesk_public_key.txt
   # Store this file somewhere safe!
   ```

3. **Document Key Location**
   - Note where your keys are stored
   - Document which `.pub` file is the correct one
   - Keep backup copies in safe location

4. **Verify After Installation**
   ```bash
   # After BetterDesk installation
   cat /opt/rustdesk/id_ed25519.pub
   # Compare with your saved copy
   ```

5. **Use BetterDesk v9+ Installation Script**
   - Newer versions include key protection
   - Automatically detects and preserves existing keys
   - Warns before any key changes

### During BetterDesk Installation

When installing BetterDesk, **always**:

✅ Choose **Option 1**: "Create automatic backup"  
✅ Select **Option 1**: "Keep existing keys" (when prompted)  
❌ Never skip backups  
❌ Avoid regenerating keys unless absolutely necessary  

---

## 🆘 Emergency Recovery

### Scenario: No Backups, Keys Lost, Clients Can't Connect

**If you have NO backups and keys are lost:**

1. **Accept the situation**: You will need to reconfigure ALL clients
2. **Generate new keys** (see Solution 4 above)
3. **Document new public key**:
   ```bash
   cat /opt/rustdesk/id_ed25519.pub | tee ~/NEW_KEY_$(date +%Y%m%d).txt
   ```

4. **Distribute to all users/devices**

5. **Reconfigure each client**:
   - Open RustDesk application
   - Click ⚙️ Settings
   - Go to **ID/Relay Server**
   - Paste new public key in **Key** field
   - Click **OK**
   - Test connection

### Scenario: BetterDesk Shows Wrong Key

**If WebConsole displays different key than expected:**

```bash
# Check what web console is reading
sudo grep "PUB_KEY_PATH" /opt/BetterDeskConsole/app.py

# Check if file exists
ls -lah /opt/rustdesk/id_ed25519.pub

# Compare keys
echo "=== File content ==="
cat /opt/rustdesk/id_ed25519.pub
echo "=== WebConsole shows ==="
# (copy from web interface)
```

**Fix**: BetterDesk v9+ automatically scans for any `.pub` file. Update to latest version:
```bash
cd /path/to/Rustdesk-FreeConsole
git pull
sudo bash install-improved.sh
```

---

## 📞 Still Having Issues?

### Diagnostic Information to Collect

Before seeking help, collect this information:

```bash
# System info
sudo bash -c 'cat <<EOF > ~/rustdesk_diagnostics.txt
=== RustDesk Diagnostics $(date) ===

--- Key Files ---
$(ls -lah /opt/rustdesk/*.pub 2>&1)
$(ls -lah /opt/rustdesk/id_ed25519 2>&1)

--- Public Key Content ---
$(cat /opt/rustdesk/id_ed25519.pub 2>&1)

--- Services Status ---
$(systemctl status rustdesksignal --no-pager 2>&1)

--- Recent Logs ---
$(journalctl -u rustdesksignal -n 30 --no-pager 2>&1)

--- Backups Available ---
$(ls -d /opt/rustdesk-backup-* 2>&1)

--- BetterDesk Version ---
$(grep "VERSION=" /opt/BetterDeskConsole/app.py 2>&1)

EOF'

cat ~/rustdesk_diagnostics.txt
```

### Where to Get Help

- 🐛 **GitHub Issues**: https://github.com/shamstabraiz/Rustdesk-FreeConsole/issues
- 💬 **RustDesk Community**: https://github.com/rustdesk/rustdesk/discussions
- 📖 **Documentation**: Check `docs/` folder in repository

---

## 📚 Additional Resources

- [RustDesk Official Docs](https://rustdesk.com/docs/)
- [SSH Key Generation Guide](https://www.ssh.com/academy/ssh/keygen)
- [BetterDesk Installation Guide](INSTALLATION_V8.md)
- [Project Documentation](PROJECT_STRUCTURE.md)

---

**Remember**: Your encryption keys are the most critical part of your RustDesk installation. Always backup before making changes!

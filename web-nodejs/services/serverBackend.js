/**
 * codenextremote Console - Server Backend Abstraction Layer
 *
 * Provides a unified interface for device/peer operations.
 * Always uses codenextremote Go server (codenextremote mode).
 *
 * Legacy 'rustdesk' (hbbs/hbbr) backend has been removed.
 * All operations delegate to codenextremoteApi.js (Go server REST API).
 *
 * The active backend is always 'codenextremote'.
 */

const config = require('../config/config');
const db = require('./database');
const codenextremoteApi = require('./codenextremoteApi');

/**
 * Return the active backend name: always 'codenextremote'
 */
async function getActiveBackend() {
    return 'codenextremote';
}

/**
 * Change the active backend. Only 'codenextremote' is supported.
 * @param {'codenextremote'} name
 */
async function setActiveBackend(name) {
    if (name !== 'codenextremote') {
        throw new Error(`Invalid backend: ${name}. Only 'codenextremote' is supported.`);
    }
    await db.setSetting('server_backend', name);
}

/**
 * Returns true — always codenextremote (Go server).
 */
async function iscodenextremote() {
    return true;
}

// ========================== Health / Stats ===================================

async function getHealth() {
    return codenextremoteApi.getHealth();
}

async function getStats() {
    const result = await codenextremoteApi.getServerStats();
    if (result.success && result.data) {
        // Normalise Go shape → panel shape
        const d = result.data;
        const total = d.peers_total ?? d.total_peers ?? d.total ?? 0;
        const online = d.peers_online ?? d.peers_online_live ?? d.online_peers ?? d.online ?? 0;
        return {
            total,
            online,
            offline: total - online,
            banned: d.peers_banned ?? d.banned_peers ?? d.banned ?? 0,
            withNotes: d.with_notes ?? 0
        };
    }
    // Fallthrough: fetch from local DB as fallback
    return await db.getStats();
}

async function getServerInfo() {
    return codenextremoteApi.getServerInfo();
}

// ========================== Devices / Peers ==================================

async function getAllDevices(filters = {}) {
    if (await iscodenextremote()) {
        let peers = await codenextremoteApi.getAllPeers();

        // Overlay folder_id from auth.db assignments (Go server doesn't track folders)
        try {
            const assignments = await db.getAllFolderAssignments();
            for (const peer of peers) {
                if (assignments[peer.id] !== undefined) {
                    peer.folder_id = assignments[peer.id];
                }
            }
        } catch (err) {
            // Non-critical: folders simply won't be assigned
            console.error('Failed to overlay folder assignments:', err.message);
        }

        // Overlay sysinfo from auth.db (Node.js receives richer data from RustDesk Client API)
        try {
            const allSysinfo = await db.getAllPeerSysinfo();
            const sysinfoMap = {};
            for (const si of allSysinfo) {
                sysinfoMap[si.peer_id] = si;
            }
            for (const peer of peers) {
                const si = sysinfoMap[peer.id];
                if (!si) continue;
                if (!peer.hostname && si.hostname) peer.hostname = si.hostname;
                if ((!peer.platform || peer.platform === '-') && si.platform) peer.platform = si.platform;
                if ((!peer.os || peer.os === '-') && si.os_full) peer.os = si.os_full;
                if (!peer.version && si.version) peer.version = si.version;
            }
        } catch (err) {
            console.error('Failed to overlay sysinfo:', err.message);
        }

        await overlayRemotePasswordMeta(peers);

        // Apply client-side filtering (the Go API may not support all filter params)
        if (filters.search) {
            const s = filters.search.toLowerCase();
            peers = peers.filter(p =>
                (p.id && p.id.toLowerCase().includes(s)) ||
                (p.username && p.username.toLowerCase().includes(s)) ||
                (p.hostname && p.hostname.toLowerCase().includes(s)) ||
                (p.note && p.note.toLowerCase().includes(s)) ||
                (p.hex_code && p.hex_code.toLowerCase().includes(s)) ||
                (p.company_id && p.company_id.toLowerCase().includes(s)) ||
                (p.signage_device_id && p.signage_device_id.toLowerCase().includes(s))
            );
        }
        if (filters.status === 'online') {
            peers = peers.filter(p => p.online);
        } else if (filters.status === 'offline') {
            peers = peers.filter(p => !p.online && !p.banned);
        } else if (filters.status === 'banned') {
            peers = peers.filter(p => p.banned);
        }
        if (filters.hasNotes) {
            peers = peers.filter(p => p.note && p.note.trim() !== '');
        }
        // Sort
        const col = filters.sortBy || 'last_online';
        const asc = filters.sortOrder === 'asc';
        peers.sort((a, b) => {
            const va = a[col] || '';
            const vb = b[col] || '';
            return asc ? String(va).localeCompare(String(vb)) : String(vb).localeCompare(String(va));
        });
        return peers;
    }
    const peers = await db.getAllDevices(filters);
    await overlayRemotePasswordMeta(peers);
    return peers;
}

async function overlayRemotePasswordMeta(peers) {
    if (!peers || !peers.length) return;
    try {
        const rows = await db.getAllPeerRemotePasswordMeta();
        const map = {};
        for (const row of rows) {
            map[row.peer_id] = row;
        }
        for (const peer of peers) {
            const meta = map[peer.id];
            peer.hex_code = meta ? (meta.hex_code || '') : '';
            peer.company_id = meta ? (meta.company_id || '') : '';
            peer.signage_device_id = meta ? (meta.signage_device_id || '') : '';
        }
    } catch (err) {
        console.error('Failed to overlay remote password meta:', err.message);
        for (const peer of peers) {
            if (peer.hex_code === undefined) peer.hex_code = '';
            if (peer.company_id === undefined) peer.company_id = '';
            if (peer.signage_device_id === undefined) peer.signage_device_id = '';
        }
    }
}

async function getDeviceById(id) {
    const peer = await codenextremoteApi.getPeer(id);
    // Overlay folder_id from auth.db
    if (peer) {
        try {
            const assignments = await db.getAllFolderAssignments();
            if (assignments[peer.id] !== undefined) {
                peer.folder_id = assignments[peer.id];
            }
        } catch { /* non-critical */ }
        try {
            const row = await db.getPeerRemotePassword(peer.id);
            peer.hex_code = row ? (row.hex_code || '') : '';
            peer.company_id = row ? (row.company_id || '') : '';
            peer.signage_device_id = row ? (row.signage_device_id || '') : '';
        } catch {
            peer.hex_code = '';
            peer.company_id = '';
            peer.signage_device_id = '';
        }
    }
    return peer;
}

async function deleteDevice(id, options = {}) {
    return codenextremoteApi.deletePeer(id, options);
}

async function setBanStatus(id, banned, reason = '') {
    return banned
        ? codenextremoteApi.banPeer(id, reason)
        : codenextremoteApi.unbanPeer(id);
}

async function updateDevice(id, data) {
    // Route through Go API PATCH /api/peers/:id for note/user/display_name fields
    const fields = {};
    if (data.note !== undefined) fields.note = String(data.note);
    if (data.user !== undefined) fields.user = String(data.user);
    if (data.display_name !== undefined) fields.display_name = String(data.display_name);

    if (Object.keys(fields).length > 0) {
        const result = await codenextremoteApi.updatePeer(id, fields);
        if (!result || !result.success) {
            return { changes: 0, error: result?.error || 'Failed to update peer' };
        }
    }

    // Also update local auth.db as fallback for overlaid fields
    try {
        await db.updateDevice(id, data);
    } catch { /* non-critical: auth.db is secondary storage */ }

    return { changes: 1 };
}

async function changePeerId(oldId, newId) {
    return codenextremoteApi.changePeerId(oldId, newId);
}

// ========================== Online Status Sync ===============================

async function syncOnlineStatus() {
    // codenextremote Go server owns the peer map — no sync needed.
    return codenextremoteApi.syncOnlineStatus();
}

// ========================== codenextremote Features ==============================

async function getStatusSummary() {
    return codenextremoteApi.getStatusSummary();
}

async function getBlocklist() {
    return codenextremoteApi.getBlocklist();
}

async function addBlocklistEntry(entry) {
    return codenextremoteApi.addBlocklistEntry(entry);
}

async function removeBlocklistEntry(entry) {
    return codenextremoteApi.removeBlocklistEntry(entry);
}

async function setPeerTags(id, tags) {
    return codenextremoteApi.setPeerTags(id, tags);
}

async function getPeersByTag(tag) {
    return codenextremoteApi.getPeersByTag(tag);
}

async function getAuditEvents(limit) {
    return codenextremoteApi.getAuditEvents(limit);
}

module.exports = {
    // Backend management
    getActiveBackend,
    setActiveBackend,
    iscodenextremote,
    // Health / Stats
    getHealth,
    getStats,
    getServerInfo,
    // Devices
    getAllDevices,
    getDeviceById,
    deleteDevice,
    setBanStatus,
    updateDevice,
    changePeerId,
    // Status sync
    syncOnlineStatus,
    // codenextremote-only
    getStatusSummary,
    getBlocklist,
    addBlocklistEntry,
    removeBlocklistEntry,
    setPeerTags,
    getPeersByTag,
    getAuditEvents
};

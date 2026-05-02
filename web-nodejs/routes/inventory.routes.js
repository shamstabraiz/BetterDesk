/**
 * BetterDesk Console — Inventory & Telemetry API Routes
 *
 * Receives hardware/software inventory and lightweight telemetry
 * data from BetterDesk desktop agents.  Data is persisted to the
 * database via dbAdapter (SQLite or PostgreSQL).
 *
 * Endpoints:
 *   POST   /api/bd/inventory   — Full inventory upload (HW + SW)
 *   POST   /api/bd/telemetry   — Lightweight telemetry (CPU/RAM)
 *   GET    /api/bd/inventory/:id — Get last inventory for a device
 *   GET    /api/inventory       — Admin endpoint: list all device inventories
 *   GET    /api/inventory/:id   — Admin endpoint: single device inventory
 *
 * @author shamstabraiz
 * @version 2.0.1
 */

'use strict';

const express = require('express');
const router = express.Router();
const db = require('../services/database');
const { getAdapter } = require('../services/dbAdapter');
const betterdeskApi = require('../services/betterdeskApi');

// ---------------------------------------------------------------------------
//  Helpers (shared with bd-api.routes.js)
// ---------------------------------------------------------------------------

function extractBearerToken(req) {
    const auth = req.headers['authorization'];
    if (!auth || !auth.startsWith('Bearer ')) return null;
    return auth.substring(7).trim();
}

/**
 * Lightweight auth — token OR X-Device-Id header.
 */
async function identifyDevice(req, res, next) {
    const token = extractBearerToken(req);
    if (token) {
        try {
            const tokenRow = await db.getAccessToken(token);
            if (tokenRow) {
                req.deviceId = tokenRow.client_id || null;
                req.deviceToken = tokenRow;
                await db.touchAccessToken(token);
                return next();
            }
        } catch (_) { /* ignored */ }
    }
    const deviceId = req.headers['x-device-id'];
    if (deviceId && /^[A-Za-z0-9_-]{3,64}$/.test(deviceId)) {
        req.deviceId = deviceId;
        return next();
    }
    return res.status(401).json({ error: 'Missing device identification' });
}

/**
 * Admin auth — require valid session (web console login).
 */
function requireAdmin(req, res, next) {
    if (req.session && req.session.user) {
        return next();
    }
    return res.status(401).json({ error: 'Admin authentication required' });
}

// ---------------------------------------------------------------------------
//  Device-facing endpoints (authenticated via token / device-id)
// ---------------------------------------------------------------------------

/**
 * POST /api/bd/inventory — Full inventory upload (HW + SW).
 *
 * Body: { device_id, hardware: {...}, software: {...}, collected_at }
 */
router.post('/inventory', identifyDevice, async (req, res) => {
    try {
        const { device_id, hardware, software, collected_at } = req.body;

        if (!device_id || !hardware) {
            return res.status(400).json({ success: false, error: 'Missing device_id or hardware' });
        }

        // Validate that the authenticated device matches
        if (req.deviceId && req.deviceId !== device_id) {
            return res.status(403).json({ success: false, error: 'Device ID mismatch' });
        }

        // Persist to database
        const adapter = getAdapter();
        await adapter.upsertInventory(device_id, hardware, software, collected_at);

        // Also update peer info in main database if peer exists
        try {
            const peer = await db.getPeerById(device_id);
            if (peer) {
                const info = {
                    ...(peer.info ? JSON.parse(peer.info) : {}),
                    hostname: hardware.hostname || undefined,
                    os: hardware.os_name || undefined,
                    os_version: hardware.os_version || undefined,
                    cpu: hardware.cpu?.brand || undefined,
                    cpu_cores: hardware.cpu?.logical_cores || undefined,
                    memory_mb: hardware.memory?.total_bytes
                        ? Math.round(hardware.memory.total_bytes / 1048576)
                        : undefined,
                };
                await db.updatePeer(device_id, { info: JSON.stringify(info) });
            }
        } catch (dbErr) {
            console.warn('[Inventory] Failed to update peer info:', dbErr.message);
        }

        console.log(`[Inventory] Full inventory received from ${device_id} (HW + ${software?.apps?.length || 0} apps)`);

        res.json({ success: true });
    } catch (err) {
        console.error('[Inventory] Upload error:', err.message);
        res.status(500).json({ success: false, error: 'Internal server error' });
    }
});

/**
 * POST /api/bd/telemetry — Lightweight telemetry (CPU/RAM/uptime).
 *
 * Body: { device_id, cpu_usage_percent, memory_used_bytes, memory_total_bytes, uptime_secs, timestamp }
 */
router.post('/telemetry', identifyDevice, async (req, res) => {
    try {
        const {
            device_id,
            cpu_usage_percent,
            memory_used_bytes,
            memory_total_bytes,
            uptime_secs,
            timestamp,
        } = req.body;

        if (!device_id) {
            return res.status(400).json({ success: false, error: 'Missing device_id' });
        }

        if (req.deviceId && req.deviceId !== device_id) {
            return res.status(403).json({ success: false, error: 'Device ID mismatch' });
        }

        const adapter = getAdapter();
        await adapter.upsertTelemetry(device_id, {
            cpu_usage_percent: cpu_usage_percent ?? 0,
            memory_used_bytes: memory_used_bytes ?? 0,
            memory_total_bytes: memory_total_bytes ?? 0,
            uptime_secs: uptime_secs ?? 0,
            timestamp: timestamp || new Date().toISOString(),
        });

        res.json({ success: true });
    } catch (err) {
        console.error('[Telemetry] Upload error:', err.message);
        res.status(500).json({ success: false, error: 'Internal server error' });
    }
});

/**
 * GET /api/bd/inventory/:id — Get last inventory for a specific device.
 * Accessible by the device itself (via token) or by admin.
 */
router.get('/inventory/:id', identifyDevice, async (req, res) => {
    try {
        const deviceId = req.params.id;
        const adapter = getAdapter();
        const entry = await adapter.getInventory(deviceId);

        if (!entry) {
            return res.status(404).json({ error: 'No inventory data for this device' });
        }

        res.json(entry);
    } catch (err) {
        console.error('[Inventory] Get error:', err.message);
        res.status(500).json({ error: 'Internal server error' });
    }
});

// ---------------------------------------------------------------------------
//  Admin-facing endpoints (web console)
// ---------------------------------------------------------------------------

/**
 * GET /api/inventory — List all device inventories (admin only).
 */
router.get('/', requireAdmin, async (req, res) => {
    try {
        const adapter = getAdapter();
        const inventories = await adapter.getAllInventories();
        const devices = [];

        if (inventories.length > 0) {
            for (const inv of inventories) {
                const telemetry = await adapter.getTelemetry(inv.device_id);
                devices.push({
                    device_id: inv.device_id,
                    hostname: inv.hardware?.hostname || inv.device_id,
                    os: `${inv.hardware?.os_name || ''} ${inv.hardware?.os_version || ''}`.trim(),
                    cpu: inv.hardware?.cpu?.brand || 'Unknown',
                    cpu_cores: inv.hardware?.cpu?.logical_cores || 0,
                    cpu_usage: telemetry?.cpu_usage_percent ?? inv.hardware?.cpu?.usage_percent ?? null,
                    memory_total_mb: inv.hardware?.memory?.total_bytes
                        ? Math.round(inv.hardware.memory.total_bytes / 1048576)
                        : 0,
                    memory_used_mb: telemetry?.memory_used_bytes
                        ? Math.round(telemetry.memory_used_bytes / 1048576)
                        : inv.hardware?.memory?.used_bytes
                            ? Math.round(inv.hardware.memory.used_bytes / 1048576)
                            : 0,
                    disk_count: inv.hardware?.disks?.length || 0,
                    software_count: inv.software?.apps?.length || 0,
                    last_seen: telemetry?.received_at || inv.received_at,
                    collected_at: inv.collected_at,
                });
            }
        } else {
            // Fallback: populate from Go server peer list when no agent inventory exists
            try {
                const peers = await betterdeskApi.getAllPeers();
                const peerList = Array.isArray(peers) ? peers : (peers?.data || []);
                for (const p of peerList) {
                    devices.push({
                        device_id: p.id,
                        hostname: p.hostname || p.id,
                        os: p.platform || p.os || '',
                        cpu: '',
                        cpu_cores: 0,
                        cpu_usage: null,
                        memory_total_mb: 0,
                        memory_used_mb: 0,
                        disk_count: 0,
                        software_count: 0,
                        last_seen: p.last_online || null,
                        collected_at: null,
                        source: 'peer_list',
                    });
                }
            } catch (fallbackErr) {
                console.warn('[Inventory] Peer list fallback failed:', fallbackErr.message);
            }
        }

        res.json({ devices, total: devices.length });
    } catch (err) {
        console.error('[Inventory] List error:', err.message);
        res.status(500).json({ error: 'Internal server error' });
    }
});

/**
 * GET /api/inventory/:id — Full inventory detail for one device (admin only).
 */
router.get('/:id', requireAdmin, async (req, res) => {
    try {
        const deviceId = req.params.id;
        const adapter = getAdapter();
        const inv = await adapter.getInventory(deviceId);
        const telemetry = await adapter.getTelemetry(deviceId);

        if (!inv) {
            return res.status(404).json({ error: 'No inventory data for this device' });
        }

        res.json({
            ...inv,
            telemetry,
        });
    } catch (err) {
        console.error('[Inventory] Detail error:', err.message);
        res.status(500).json({ error: 'Internal server error' });
    }
});

module.exports = router;

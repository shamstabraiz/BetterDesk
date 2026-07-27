/**
 * Yomie Console - Remote Desktop Routes
 * Serves the web-based remote desktop viewer page (RustDesk compat + Yomie native)
 */

const express = require('express');
const router = express.Router();
const fs = require('fs');
const db = require('../services/database');
const config = require('../config/config');
const { requireAuth, assertSignageAllowsPeer, getSignageControl } = require('../middleware/auth');
const signageControlLinks = require('../services/signageControlLinks');

// Lazy-loaded relay helper — avoid circular require at module load time
function getRemoteRelay() {
    try { return require('../services/remoteRelay'); } catch { return null; }
}

// Read server public key once at startup
let serverPubKey = '';
try {
    if (fs.existsSync(config.pubKeyPath)) {
        serverPubKey = fs.readFileSync(config.pubKeyPath, 'utf8').trim();
    }
} catch (err) {
    console.warn('Warning: Could not read server public key:', err.message);
}

/**
 * GET /remote - Redirect to devices page (device ID required for remote)
 */
router.get('/remote', requireAuth, (req, res) => {
    res.redirect('/devices');
});

/**
 * GET /remote/signage/:signageDeviceId — Redeem a short-lived control-link token
 * and open the unified remote viewer for the resolved peer (no panel login).
 *
 * Query: ?token=<opaque hex>
 * On success: scoped guest session + 302 → /remote/:peerId
 */
router.get('/remote/signage/:signageDeviceId', async (req, res) => {
    const signageDeviceId = String(req.params.signageDeviceId || '').trim();
    const token = String(req.query.token || '').trim();

    if (!signageDeviceId || signageDeviceId.length > 128) {
        return res.status(400).send('Invalid signage device id');
    }
    if (!token) {
        return res.status(400).send('Missing token');
    }

    try {
        const redeemed = await signageControlLinks.redeem(signageDeviceId, token);
        const sessionFields = signageControlLinks.buildSignageSession(redeemed);

        // Clear any prior panel identity; this session is scoped to one peer only.
        req.session.userId = sessionFields.userId;
        req.session.user = sessionFields.user;
        req.session.signageControl = sessionFields.signageControl;

        await new Promise((resolve, reject) => {
            req.session.save((err) => (err ? reject(err) : resolve()));
        });

        return res.redirect(`/remote/${encodeURIComponent(redeemed.peerId)}`);
    } catch (err) {
        const status = err.status || 403;
        console.warn(`[remote] signage redeem failed: ${err.code || err.message}`);
        return res.status(status).send(err.message || 'Control link rejected');
    }
});

/**
 * GET /remote/:deviceId - Unified remote desktop viewer (single entry point).
 *
 * Phase 2.1 of the unification plan: this route is now the only canonical
 * URL for browser-based remote desktop. The transport (RustDesk relay vs.
 * CDAP WebSocket) is auto-detected on the server by probing the Go server
 * for `device_type` and `cdap_connected`. The decision is then passed to
 * the appropriate template.
 *
 * Query overrides:
 *   ?transport=cdap   → force CDAP transport (skip auto-probe)
 *   ?transport=rd     → force RustDesk transport
 *
 * Until the unified `remote.ejs` shell lands (PR 2.2 / 2.3) we still render
 * the existing two templates underneath. Operators get a single URL and
 * shareable links work regardless of which transport is active.
 */
router.get('/remote/:deviceId', requireAuth, async (req, res) => {
    const deviceId = req.params.deviceId;

    if (!deviceId || !/^[A-Za-z0-9_-]{3,64}$/.test(deviceId)) {
        return res.redirect('/devices');
    }

    // Signage guest sessions may only open their granted peer.
    if (req.session.signageControl) {
        const sc = getSignageControl(req);
        if (!sc || !assertSignageAllowsPeer(req, deviceId)) {
            await new Promise((resolve) => {
                req.session.destroy(() => resolve());
            });
            return res.status(403).send('Signage control session expired or not authorized for this device');
        }
    }

    let device = null;
    try {
        device = await db.getDevice(deviceId);
    } catch {
        // Database lookup failure is non-blocking - viewer can still work
    }

    // Probe Go server for authoritative transport hint. Local panel DB
    // does not carry `device_type` or `cdap_connected`.
    let isOsAgent = false;
    let isCdapConnected = false;
    let goPeer = null;
    try {
        const api = require('../services/yomieApi');
        goPeer = await api.getPeer(deviceId);
        if (goPeer) {
            isOsAgent = String(goPeer.device_type || '').toLowerCase() === 'os_agent';
            isCdapConnected = !!goPeer.cdap_connected;
        }
    } catch { /* non-fatal: degrade to standard viewer */ }

    // Resolve transport: explicit query param wins, then auto-detect.
    const forced = String(req.query.transport || '').toLowerCase();
    let transport;
    if (forced === 'cdap' || forced === 'rd') {
        transport = forced;
    } else if (isOsAgent || isCdapConnected) {
        transport = 'cdap';
    } else {
        transport = 'rd';
    }

    // Capability hints exposed to the browser so the unified UI can light
    // up the right toolbar buttons.
    const capabilities = {
        transport,
        os_agent: isOsAgent,
        cdap_connected: isCdapConnected,
        device_type: goPeer && goPeer.device_type ? String(goPeer.device_type) : '',
    };

    // PR 2.2/2.3 unification: a single canonical web client (`remote.ejs`)
    // serves both transports. The browser branches on
    // `window.__capabilities.transport`. The legacy `remote-cdap` template
    // is no longer rendered; its inline widget remains usable from
    // device-detail panels via `cdap-desktop.js` directly.
    res.render('remote', {
        title: `${req.t('remote.title')} - ${deviceId}`,
        activePage: 'remote',
        deviceId: deviceId,
        device: device || { id: deviceId, hostname: '', platform: '', note: '' },
        serverPubKey: serverPubKey,
        capabilities,
        layout: 'viewer'
    });
});

/**
 * GET /remote-cdap/:deviceId - Legacy alias, redirects to unified entry.
 *
 * Kept for backwards compatibility with existing bookmarks, deep links, and
 * the `devices.js` "Connect" button. New code should link to
 * `/remote/:deviceId` directly.
 */
router.get('/remote-cdap/:deviceId', requireAuth, (req, res) => {
    const deviceId = req.params.deviceId;
    if (deviceId && /^[A-Za-z0-9_-]{3,64}$/.test(deviceId)) {
        return res.redirect(`/remote/${encodeURIComponent(deviceId)}?transport=cdap`);
    }
    return res.redirect('/devices');
});

/**
 * GET /remote-desktop/:deviceId - Legacy route, redirects to unified /remote/:deviceId
 *
 * Previously served a separate JPEG stream viewer. The web remote client has
 * been unified: `/remote/:deviceId` is now the only canonical entry point for
 * browser-based remote desktop.
 */
router.get('/remote-desktop/:deviceId', requireAuth, (req, res) => {
    const deviceId = req.params.deviceId;
    if (deviceId && /^[A-Za-z0-9_-]{3,64}$/.test(deviceId)) {
        return res.redirect(`/remote/${encodeURIComponent(deviceId)}`);
    }
    return res.redirect('/devices');
});

/**
 * GET /api/remote/sessions - List active native remote sessions
 */
router.get('/api/remote/sessions', requireAuth, (req, res) => {
    const relay = getRemoteRelay();
    if (!relay) return res.json({ sessions: [] });
    const sessions = relay.getActiveSessions();
    res.json({ sessions });
});

/**
 * GET /api/remote/session/:deviceId - Get state of a single native remote session
 */
router.get('/api/remote/session/:deviceId', requireAuth, (req, res) => {
    const relay = getRemoteRelay();
    if (!relay) return res.status(404).json({ error: 'Remote relay not available' });
    const state = relay.getSessionState(req.params.deviceId);
    if (!state) return res.status(404).json({ error: 'Session not found' });
    res.json(state);
});

/**
 * POST /api/diag/remote-file — browser reports in-session file-transfer diagnostics.
 * FileAction traffic is peer-to-peer (relay), so the console only sees these
 * events if the web client posts them. Used to confirm browse timeout vs reply.
 */
router.post('/api/diag/remote-file', requireAuth, (req, res) => {
    const body = req.body || {};
    const event = String(body.event || 'unknown').slice(0, 64);
    const deviceId = String(body.deviceId || '-').slice(0, 64);
    const path = body.path != null ? String(body.path).slice(0, 512) : '';
    const detail = body.detail != null ? String(body.detail).slice(0, 500) : '';
    const user = (req.session && (req.session.username || req.session.userId)) || '-';
    console.log(
        `[DIAG:remote-file] event=${event} device=${deviceId} user=${user}` +
        (path !== '' ? ` path=${JSON.stringify(path)}` : '') +
        (detail ? ` detail=${JSON.stringify(detail)}` : '')
    );
    res.json({ ok: true });
});

module.exports = router;

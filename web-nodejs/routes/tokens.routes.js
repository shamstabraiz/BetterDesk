/**
 * Yomie Console - Device Token Routes
 * Proxy routes for Go server device token management API.
 */

const express = require('express');
const router = express.Router();
const { requireAuth, requirePermission } = require('../middleware/auth');
const yomieApi = require('../services/yomieApi');

// ── Page Route ───────────────────────────────────────────────────────────

router.get('/tokens', requireAuth, requirePermission('enrollment.manage'), (req, res) => {
    res.render('tokens', {
        title: req.t('tokens.title'),
        activePage: 'tokens',
        currentPage: 'tokens'
    });
});

// ── API Proxy Routes ─────────────────────────────────────────────────────

// List all tokens
router.get('/api/panel/tokens', requireAuth, requirePermission('enrollment.manage'), async (req, res) => {
    try {
        const includeRevoked = req.query.include_revoked === 'true';
        const result = await yomieApi.listDeviceTokens(includeRevoked);
        res.json(result);
    } catch (err) {
        res.status(500).json({ success: false, error: 'Failed to list tokens' });
    }
});

// Create token
router.post('/api/panel/tokens', requireAuth, requirePermission('enrollment.manage'), async (req, res) => {
    try {
        const result = await yomieApi.createDeviceToken(req.body);
        res.json(result);
    } catch (err) {
        res.status(500).json({ success: false, error: 'Failed to create token' });
    }
});

// Bulk generate tokens
router.post('/api/panel/tokens/generate-bulk', requireAuth, requirePermission('enrollment.manage'), async (req, res) => {
    try {
        const result = await yomieApi.bulkGenerateTokens(req.body);
        res.json(result);
    } catch (err) {
        res.status(500).json({ success: false, error: 'Failed to generate tokens' });
    }
});

// Update token
router.put('/api/panel/tokens/:id', requireAuth, requirePermission('enrollment.manage'), async (req, res) => {
    try {
        const result = await yomieApi.updateDeviceToken(req.params.id, req.body);
        res.json(result);
    } catch (err) {
        res.status(500).json({ success: false, error: 'Failed to update token' });
    }
});

// Revoke token
router.delete('/api/panel/tokens/:id', requireAuth, requirePermission('enrollment.manage'), async (req, res) => {
    try {
        const result = await yomieApi.revokeDeviceToken(req.params.id);
        res.json(result);
    } catch (err) {
        res.status(500).json({ success: false, error: 'Failed to revoke token' });
    }
});

// Bind token to peer
router.post('/api/panel/tokens/:id/bind', requireAuth, requirePermission('enrollment.manage'), async (req, res) => {
    try {
        const result = await yomieApi.bindTokenToPeer(req.params.id, req.body.peer_id);
        res.json(result);
    } catch (err) {
        res.status(500).json({ success: false, error: 'Failed to bind token' });
    }
});

// Get enrollment mode
router.get('/api/panel/enrollment/mode', requireAuth, requirePermission('enrollment.manage'), async (req, res) => {
    try {
        const result = await yomieApi.getEnrollmentMode();
        res.json(result);
    } catch (err) {
        res.status(500).json({ success: false, error: 'Failed to get enrollment mode' });
    }
});

// Set enrollment mode
router.put('/api/panel/enrollment/mode', requireAuth, requirePermission('enrollment.manage'), async (req, res) => {
    try {
        const result = await yomieApi.setEnrollmentMode(req.body.mode);
        res.json(result);
    } catch (err) {
        res.status(500).json({ success: false, error: 'Failed to set enrollment mode' });
    }
});

module.exports = router;

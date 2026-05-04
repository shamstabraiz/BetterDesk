'use strict';

const express = require('express');
const router = express.Router();
const { requireAuth, requirePermission } = require('../middleware/auth');
const { apiClient } = require('../services/yomieApi');

// ---------------------------------------------------------------------------
// Helper — proxy to Go server
// ---------------------------------------------------------------------------
async function goApiProxy(req, res, method, path, body) {
    try {
        const opts = { method, url: path };
        if (body) opts.data = body;
        const resp = await apiClient(opts);
        res.status(resp.status).json(resp.data);
    } catch (err) {
        const status = err.response?.status || 500;
        const data = err.response?.data || { error: 'Go server unreachable' };
        res.status(status).json(data);
    }
}

// ---------------------------------------------------------------------------
// Page route
// ---------------------------------------------------------------------------

router.get('/scaling', requireAuth, requirePermission('server.config'), (req, res) => {
    const tab = req.query.tab || 'overview';
    res.render('scaling', {
        title: req.t('scaling.title'),
        pageStyles: ['scaling'],
        pageScripts: ['scaling'],
        currentPage: 'scaling',
        breadcrumb: [{ label: req.t('scaling.title') }],
        activeTab: tab
    });
});

// ---------------------------------------------------------------------------
// Relay Nodes API
// ---------------------------------------------------------------------------

// List all registered relay nodes
router.get('/api/panel/scaling/relays', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'GET', '/scaling/relays');
});

// Get single relay node details
router.get('/api/panel/scaling/relays/:nodeId', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'GET', `/scaling/relays/${encodeURIComponent(req.params.nodeId)}`);
});

// Register / add a relay node
router.post('/api/panel/scaling/relays', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'POST', '/scaling/relays', req.body);
});

// Update relay node config
router.put('/api/panel/scaling/relays/:nodeId', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'PUT', `/scaling/relays/${encodeURIComponent(req.params.nodeId)}`, req.body);
});

// Remove relay node
router.delete('/api/panel/scaling/relays/:nodeId', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'DELETE', `/scaling/relays/${encodeURIComponent(req.params.nodeId)}`);
});

// Relay node health data
router.get('/api/panel/scaling/relays/:nodeId/health', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'GET', `/scaling/relays/${encodeURIComponent(req.params.nodeId)}/health`);
});

// ---------------------------------------------------------------------------
// Capacity & Metrics
// ---------------------------------------------------------------------------

// Cluster-wide capacity overview
router.get('/api/panel/scaling/capacity', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'GET', '/scaling/capacity');
});

// Bandwidth / load history for a relay
router.get('/api/panel/scaling/relays/:nodeId/metrics', requireAuth, requirePermission('server.config'), (req, res) => {
    const qs = req.query.period ? `?period=${encodeURIComponent(req.query.period)}` : '';
    goApiProxy(req, res, 'GET', `/scaling/relays/${encodeURIComponent(req.params.nodeId)}/metrics${qs}`);
});

// ---------------------------------------------------------------------------
// Relay Assignment Rules
// ---------------------------------------------------------------------------

// List assignment rules
router.get('/api/panel/scaling/rules', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'GET', '/scaling/rules');
});

// Create assignment rule
router.post('/api/panel/scaling/rules', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'POST', '/scaling/rules', req.body);
});

// Update assignment rule
router.put('/api/panel/scaling/rules/:ruleId', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'PUT', `/scaling/rules/${encodeURIComponent(req.params.ruleId)}`, req.body);
});

// Delete assignment rule
router.delete('/api/panel/scaling/rules/:ruleId', requireAuth, requirePermission('server.config'), (req, res) => {
    goApiProxy(req, res, 'DELETE', `/scaling/rules/${encodeURIComponent(req.params.ruleId)}`);
});

// ---------------------------------------------------------------------------
// Device-facing: relay node heartbeat (from relay nodes themselves)
// ---------------------------------------------------------------------------
router.post('/api/bd/scaling/relay-heartbeat', (req, res) => {
    goApiProxy(req, res, 'POST', '/scaling/relay-heartbeat', req.body);
});

module.exports = router;

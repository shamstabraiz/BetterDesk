/**
 * BetterDesk Console — Organization Management Routes (v3.0.0)
 *
 * Proxies organization CRUD operations to the Go server REST API.
 * Provides page routes for the web panel and API routes for AJAX calls.
 *
 * Page routes:
 *   GET /organizations          — Organizations management page
 *   GET /organizations/:id      — Organization detail page
 *
 * API routes (proxy to Go server /api/org/*):
 *   GET    /api/panel/org              — List organizations
 *   POST   /api/panel/org              — Create organization
 *   GET    /api/panel/org/:id          — Get organization
 *   PUT    /api/panel/org/:id          — Update organization
 *   DELETE /api/panel/org/:id          — Delete organization
 *   GET    /api/panel/org/:id/users    — List org users
 *   POST   /api/panel/org/:id/users    — Create org user
 *   PUT    /api/panel/org/:id/users/:uid — Update org user
 *   DELETE /api/panel/org/:id/users/:uid — Delete org user
 *   POST   /api/panel/org/:id/invite   — Create invitation
 *   GET    /api/panel/org/:id/invitations — List invitations
 *   POST   /api/panel/org/:id/devices  — Assign device to org
 *   GET    /api/panel/org/:id/devices  — List org devices
 *   DELETE /api/panel/org/:id/devices/:did — Unassign device
 *   GET    /api/panel/org/:id/settings — List org settings
 *   PUT    /api/panel/org/:id/settings — Set org setting
 */

'use strict';

const express = require('express');
const router = express.Router();
const { apiClient } = require('../services/betterdeskApi');
const { requireAuth, requirePermission } = require('../middleware/auth');
const userSync = require('../services/userSync');

// ---------------------------------------------------------------------------
//  Helper: proxy to Go server
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

async function resolveGoMemberId(userId) {
    const resolved = await userSync.resolveGoUserId(userId);
    return resolved || userId;
}

// ---------------------------------------------------------------------------
//  Page routes
// ---------------------------------------------------------------------------

router.get('/organizations', requireAuth, (req, res) => {
    res.render('organizations', {
        title: 'Organizations',
        user: req.session.user,
        currentPage: 'organizations',
    });
});

router.get('/organizations/:id', requireAuth, (req, res) => {
    res.render('organization-detail', {
        title: 'Organization Details',
        user: req.session.user,
        currentPage: 'organizations',
        orgId: req.params.id,
    });
});

// ---------------------------------------------------------------------------
//  API routes (proxy to Go server)
// ---------------------------------------------------------------------------

// Organizations CRUD
router.get('/api/panel/org', requireAuth, (req, res) => goApiProxy(req, res, 'get', '/org'));
router.post('/api/panel/org', requireAuth, requirePermission('org.create'), (req, res) => goApiProxy(req, res, 'post', '/org', req.body));
router.get('/api/panel/org/:id', requireAuth, (req, res) => goApiProxy(req, res, 'get', `/org/${req.params.id}`));
router.put('/api/panel/org/:id', requireAuth, requirePermission('org.edit'), (req, res) => goApiProxy(req, res, 'put', `/org/${req.params.id}`, req.body));
router.delete('/api/panel/org/:id', requireAuth, requirePermission('org.delete'), (req, res) => goApiProxy(req, res, 'delete', `/org/${req.params.id}`));

// Org Users
router.get('/api/panel/org/:id/users', requireAuth, (req, res) => goApiProxy(req, res, 'get', `/org/${req.params.id}/users`));
router.post('/api/panel/org/:id/users', requireAuth, requirePermission('org.manage_users'), (req, res) => goApiProxy(req, res, 'post', `/org/${req.params.id}/users`, req.body));
router.put('/api/panel/org/:id/users/:uid', requireAuth, requirePermission('org.manage_users'), (req, res) => goApiProxy(req, res, 'put', `/org/${req.params.id}/users/${req.params.uid}`, req.body));
router.delete('/api/panel/org/:id/users/:uid', requireAuth, requirePermission('org.manage_users'), (req, res) => goApiProxy(req, res, 'delete', `/org/${req.params.id}/users/${req.params.uid}`));

// User-Org Linking (Issue #106)
router.get('/api/panel/org/:id/available-users', requireAuth, requirePermission('org.manage_users'), (req, res) => goApiProxy(req, res, 'get', `/org/${req.params.id}/available-users`));
router.post('/api/panel/org/:id/members', requireAuth, requirePermission('org.manage_users'), (req, res) => goApiProxy(req, res, 'post', `/org/${req.params.id}/members`, req.body));
router.delete('/api/panel/org/:id/members/:userId', requireAuth, requirePermission('org.manage_users'), async (req, res) => {
    try {
        const goUserId = await resolveGoMemberId(req.params.userId);
        goApiProxy(req, res, 'delete', `/org/${req.params.id}/members/${goUserId}`);
    } catch (err) {
        res.status(500).json({ error: 'Failed to resolve user' });
    }
});

// Invitations
router.post('/api/panel/org/:id/invite', requireAuth, requirePermission('org.manage_users'), (req, res) => goApiProxy(req, res, 'post', `/org/${req.params.id}/invite`, req.body));
router.get('/api/panel/org/:id/invitations', requireAuth, requirePermission('org.manage_users'), (req, res) => goApiProxy(req, res, 'get', `/org/${req.params.id}/invitations`));

// Devices
router.post('/api/panel/org/:id/devices', requireAuth, requirePermission('org.manage_devices'), (req, res) => goApiProxy(req, res, 'post', `/org/${req.params.id}/devices`, req.body));
router.get('/api/panel/org/:id/devices', requireAuth, (req, res) => goApiProxy(req, res, 'get', `/org/${req.params.id}/devices`));
router.delete('/api/panel/org/:id/devices/:did', requireAuth, requirePermission('org.manage_devices'), (req, res) => goApiProxy(req, res, 'delete', `/org/${req.params.id}/devices/${req.params.did}`));

// Settings
router.get('/api/panel/org/:id/settings', requireAuth, (req, res) => goApiProxy(req, res, 'get', `/org/${req.params.id}/settings`));
router.put('/api/panel/org/:id/settings', requireAuth, requirePermission('org.edit'), (req, res) => goApiProxy(req, res, 'put', `/org/${req.params.id}/settings`, req.body));

module.exports = router;

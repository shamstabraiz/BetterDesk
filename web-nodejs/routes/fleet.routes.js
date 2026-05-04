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
// Page routes
// ---------------------------------------------------------------------------

// Fleet management dashboard (tabs: resources, tasks, builder, compliance)
router.get('/fleet', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    const tab = req.query.tab || 'resources';
    res.render('fleet', {
        title: req.t('fleet.title'),
        pageStyles: ['fleet'],
        pageScripts: ['fleet', 'fleet-builder'],
        currentPage: 'fleet',
        breadcrumb: [{ label: req.t('fleet.title') }],
        activeTab: tab
    });
});

// ---------------------------------------------------------------------------
// Resource Mapping API
// ---------------------------------------------------------------------------

// List resource mappings for an org
router.get('/api/panel/fleet/resources/:orgId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'GET', `/fleet/resources/${encodeURIComponent(req.params.orgId)}`);
});

// Create a resource mapping
router.post('/api/panel/fleet/resources/:orgId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'POST', `/fleet/resources/${encodeURIComponent(req.params.orgId)}`, req.body);
});

// Update a resource mapping
router.put('/api/panel/fleet/resources/:orgId/:resourceId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    const { orgId, resourceId } = req.params;
    goApiProxy(req, res, 'PUT', `/fleet/resources/${encodeURIComponent(orgId)}/${encodeURIComponent(resourceId)}`, req.body);
});

// Delete a resource mapping
router.delete('/api/panel/fleet/resources/:orgId/:resourceId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    const { orgId, resourceId } = req.params;
    goApiProxy(req, res, 'DELETE', `/fleet/resources/${encodeURIComponent(orgId)}/${encodeURIComponent(resourceId)}`);
});

// ---------------------------------------------------------------------------
// Task Scheduler API
// ---------------------------------------------------------------------------

// List tasks (optional filters: ?orgId=X&status=pending&deviceId=Y)
router.get('/api/panel/fleet/tasks', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    const qs = new URLSearchParams();
    if (req.query.orgId) qs.set('org_id', req.query.orgId);
    if (req.query.status) qs.set('status', req.query.status);
    if (req.query.deviceId) qs.set('device_id', req.query.deviceId);
    goApiProxy(req, res, 'GET', `/fleet/tasks?${qs.toString()}`);
});

// Get single task
router.get('/api/panel/fleet/tasks/:taskId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'GET', `/fleet/tasks/${encodeURIComponent(req.params.taskId)}`);
});

// Create a task
router.post('/api/panel/fleet/tasks', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'POST', '/fleet/tasks', {
        ...req.body,
        created_by: req.session.user?.username || 'admin'
    });
});

// Update a task
router.put('/api/panel/fleet/tasks/:taskId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'PUT', `/fleet/tasks/${encodeURIComponent(req.params.taskId)}`, req.body);
});

// Cancel / delete a task
router.delete('/api/panel/fleet/tasks/:taskId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'DELETE', `/fleet/tasks/${encodeURIComponent(req.params.taskId)}`);
});

// Get task execution output log
router.get('/api/panel/fleet/tasks/:taskId/output', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'GET', `/fleet/tasks/${encodeURIComponent(req.params.taskId)}/output`);
});

// Retry a failed task
router.post('/api/panel/fleet/tasks/:taskId/retry', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'POST', `/fleet/tasks/${encodeURIComponent(req.params.taskId)}/retry`);
});

// ---------------------------------------------------------------------------
// Workflow / Visual Builder API
// ---------------------------------------------------------------------------

// List workflows
router.get('/api/panel/fleet/workflows', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'GET', '/fleet/workflows');
});

// Get single workflow
router.get('/api/panel/fleet/workflows/:workflowId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'GET', `/fleet/workflows/${encodeURIComponent(req.params.workflowId)}`);
});

// Save workflow
router.post('/api/panel/fleet/workflows', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'POST', '/fleet/workflows', {
        ...req.body,
        created_by: req.session.user?.username || 'admin'
    });
});

// Update workflow
router.put('/api/panel/fleet/workflows/:workflowId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'PUT', `/fleet/workflows/${encodeURIComponent(req.params.workflowId)}`, req.body);
});

// Delete workflow
router.delete('/api/panel/fleet/workflows/:workflowId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'DELETE', `/fleet/workflows/${encodeURIComponent(req.params.workflowId)}`);
});

// Execute a workflow (dry-run or real)
router.post('/api/panel/fleet/workflows/:workflowId/execute', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'POST', `/fleet/workflows/${encodeURIComponent(req.params.workflowId)}/execute`, req.body);
});

// Workflow execution history
router.get('/api/panel/fleet/workflows/:workflowId/history', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'GET', `/fleet/workflows/${encodeURIComponent(req.params.workflowId)}/history`);
});

// ---------------------------------------------------------------------------
// Compliance API
// ---------------------------------------------------------------------------

// Compliance overview (per org)
router.get('/api/panel/fleet/compliance', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    const qs = new URLSearchParams();
    if (req.query.orgId) qs.set('org_id', req.query.orgId);
    goApiProxy(req, res, 'GET', `/fleet/compliance?${qs.toString()}`);
});

// Per-device compliance details
router.get('/api/panel/fleet/compliance/:deviceId', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'GET', `/fleet/compliance/${encodeURIComponent(req.params.deviceId)}`);
});

// Trigger compliance scan on a device
router.post('/api/panel/fleet/compliance/:deviceId/scan', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'POST', `/fleet/compliance/${encodeURIComponent(req.params.deviceId)}/scan`);
});

// Remediation action on a device
router.post('/api/panel/fleet/compliance/:deviceId/remediate', requireAuth, requirePermission('org.manage_devices'), (req, res) => {
    goApiProxy(req, res, 'POST', `/fleet/compliance/${encodeURIComponent(req.params.deviceId)}/remediate`, req.body);
});

// ---------------------------------------------------------------------------
// Device-facing API (agents report back)
// ---------------------------------------------------------------------------

// Agent reports task completion
router.post('/api/bd/fleet/task-result', (req, res) => {
    goApiProxy(req, res, 'POST', '/fleet/task-result', req.body);
});

// Agent reports installed software inventory
router.post('/api/bd/fleet/software', (req, res) => {
    goApiProxy(req, res, 'POST', '/fleet/software', req.body);
});

module.exports = router;

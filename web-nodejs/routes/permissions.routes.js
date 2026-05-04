/**
 * Yomie Console - Permissions Routes (RBAC Phase 52)
 * Role and permission management for admins.
 */

const express = require('express');
const router = express.Router();
const { requireAuth, requirePermission } = require('../middleware/auth');
const yomieApi = require('../services/yomieApi');

// ── Page Route ───────────────────────────────────

/**
 * GET /permissions - Permissions management page
 */
router.get('/permissions', requireAuth, requirePermission('server.config'), (req, res) => {
    res.render('permissions', {
        title: req.t('permissions.title'),
        activePage: 'permissions',
        currentPage: 'permissions'
    });
});

// ── API Proxy Routes ─────────────────────────────

/**
 * GET /api/panel/roles - List all roles with permissions
 */
router.get('/api/panel/roles', requireAuth, requirePermission('user.view'), async (req, res) => {
    try {
        const result = await yomieApi.listRoles();
        if (!result.success) {
            return res.status(500).json({ success: false, error: result.error });
        }
        res.json(result);
    } catch (err) {
        console.error('Failed to list roles:', err);
        res.status(500).json({ success: false, error: 'Failed to list roles' });
    }
});

/**
 * GET /api/panel/roles/:role/permissions - Get effective permissions for a role
 */
router.get('/api/panel/roles/:role/permissions', requireAuth, requirePermission('user.view'), async (req, res) => {
    try {
        const result = await yomieApi.getRolePermissions(req.params.role);
        if (!result.success) {
            return res.status(500).json({ success: false, error: result.error });
        }
        res.json(result);
    } catch (err) {
        console.error('Failed to get role permissions:', err);
        res.status(500).json({ success: false, error: 'Failed to get role permissions' });
    }
});

/**
 * GET /api/panel/role-permissions - List custom overrides
 */
router.get('/api/panel/role-permissions', requireAuth, requirePermission('server.config'), async (req, res) => {
    try {
        const result = await yomieApi.listRolePermissionOverrides(req.query.role);
        if (!result.success) {
            return res.status(500).json({ success: false, error: result.error });
        }
        res.json(result);
    } catch (err) {
        console.error('Failed to list permission overrides:', err);
        res.status(500).json({ success: false, error: 'Failed to list permission overrides' });
    }
});

/**
 * POST /api/panel/role-permissions - Set a custom override
 */
router.post('/api/panel/role-permissions', requireAuth, requirePermission('server.config'), async (req, res) => {
    try {
        const { role, permission, granted } = req.body;
        if (!role || !permission || typeof granted !== 'boolean') {
            return res.status(400).json({ success: false, error: 'Missing required fields: role, permission, granted' });
        }
        const result = await yomieApi.setRolePermission(role, permission, granted);
        if (!result.success) {
            return res.status(400).json({ success: false, error: result.error || 'Failed to set permission' });
        }
        res.json(result);
    } catch (err) {
        console.error('Failed to set role permission:', err);
        res.status(500).json({ success: false, error: 'Failed to set role permission' });
    }
});

/**
 * DELETE /api/panel/role-permissions/:role/:permission - Delete a custom override
 */
router.delete('/api/panel/role-permissions/:role/:permission', requireAuth, requirePermission('server.config'), async (req, res) => {
    try {
        const result = await yomieApi.deleteRolePermission(req.params.role, req.params.permission);
        if (!result.success) {
            return res.status(400).json({ success: false, error: result.error || 'Failed to delete override' });
        }
        res.json(result);
    } catch (err) {
        console.error('Failed to delete role permission:', err);
        res.status(500).json({ success: false, error: 'Failed to delete role permission' });
    }
});

module.exports = router;

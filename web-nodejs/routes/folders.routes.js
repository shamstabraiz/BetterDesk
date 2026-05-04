/**
 * Yomie Console - Folders Routes
 * Device folder organization
 */

const express = require('express');
const router = express.Router();
const db = require('../services/database');
const serverBackend = require('../services/serverBackend');
const { requireAuth } = require('../middleware/auth');

/**
 * GET /api/folders - Get all folders
 */
router.get('/api/folders', requireAuth, async (req, res) => {
    try {
        const folders = await db.getAllFolders();

        // Enrich each folder with device_count from assignments table
        try {
            const assignments = await db.getAllFolderAssignments();
            const countMap = {};
            for (const [, folderId] of Object.entries(assignments)) {
                countMap[folderId] = (countMap[folderId] || 0) + 1;
            }
            for (const f of folders) {
                f.device_count = countMap[f.id] || 0;
            }
        } catch (err) {
            console.error('Failed to compute folder device counts:', err.message);
        }

        res.json({
            success: true,
            data: {
                folders,
                total: folders.length
            }
        });
    } catch (err) {
        console.error('Get folders error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * POST /api/folders - Create new folder
 */
router.post('/api/folders', requireAuth, async (req, res) => {
    try {
        const { name, color, icon } = req.body;
        
        if (!name || name.trim().length === 0) {
            return res.status(400).json({
                success: false,
                error: req.t('folders.name_required')
            });
        }
        
        if (name.length > 50) {
            return res.status(400).json({
                success: false,
                error: req.t('folders.name_too_long')
            });
        }
        
        const result = await db.createFolder(name.trim(), color || '#6366f1', icon || 'folder');
        
        // Log action
        await db.logAction(req.session.userId, 'folder_created', `Created folder: ${name}`, req.ip);
        
        res.json({
            success: true,
            data: {
                id: result.id,
                name: name.trim(),
                color: color || '#6366f1',
                icon: icon || 'folder'
            }
        });
    } catch (err) {
        console.error('Create folder error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * PATCH /api/folders/:id - Update folder
 */
router.patch('/api/folders/:id', requireAuth, async (req, res) => {
    try {
        const folderId = parseInt(req.params.id, 10);
        const { name, color, icon } = req.body;
        
        const folder = await db.getFolderById(folderId);
        if (!folder) {
            return res.status(404).json({
                success: false,
                error: req.t('folders.not_found')
            });
        }
        
        if (name !== undefined) {
            if (name.trim().length === 0) {
                return res.status(400).json({
                    success: false,
                    error: req.t('folders.name_required')
                });
            }
            if (name.length > 50) {
                return res.status(400).json({
                    success: false,
                    error: req.t('folders.name_too_long')
                });
            }
        }
        
        await db.updateFolder(folderId, {
            name: name !== undefined ? name.trim() : undefined,
            color,
            icon
        });
        
        // Log action
        await db.logAction(req.session.userId, 'folder_updated', `Updated folder: ${name || folder.name}`, req.ip);
        
        res.json({ success: true });
    } catch (err) {
        console.error('Update folder error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * DELETE /api/folders/:id - Delete folder
 */
router.delete('/api/folders/:id', requireAuth, async (req, res) => {
    try {
        const folderId = parseInt(req.params.id, 10);
        
        const folder = await db.getFolderById(folderId);
        if (!folder) {
            return res.status(404).json({
                success: false,
                error: req.t('folders.not_found')
            });
        }
        
        // Remove folder assignment from devices
        await db.unassignDevicesFromFolder(folderId);
        
        // Delete folder
        await db.deleteFolder(folderId);
        
        // Log action
        await db.logAction(req.session.userId, 'folder_deleted', `Deleted folder: ${folder.name}`, req.ip);
        
        res.json({ success: true });
    } catch (err) {
        console.error('Delete folder error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * POST /api/folders/:id/devices - Assign devices to folder
 */
router.post('/api/folders/:id/devices', requireAuth, async (req, res) => {
    try {
        const folderId = parseInt(req.params.id, 10);
        const { deviceIds } = req.body;
        
        if (!Array.isArray(deviceIds) || deviceIds.length === 0) {
            return res.status(400).json({
                success: false,
                error: req.t('folders.no_devices_selected')
            });
        }
        
        // Verify folder exists (null = unassign)
        if (folderId !== 0) {
            const folder = await db.getFolderById(folderId);
            if (!folder) {
                return res.status(404).json({
                    success: false,
                    error: req.t('folders.not_found')
                });
            }
        }
        
        const assignFolderId = folderId === 0 ? null : folderId;
        
        await db.assignDevicesToFolder(deviceIds, assignFolderId);
        
        // Log action
        await db.logAction(req.session.userId, 'devices_moved', 
            `Moved ${deviceIds.length} device(s) to folder ID: ${folderId}`, req.ip);
        
        res.json({
            success: true,
            data: { count: deviceIds.length }
        });
    } catch (err) {
        console.error('Assign devices error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * PATCH /api/devices/:id/folder - Assign single device to folder
 */
router.patch('/api/devices/:id/folder', requireAuth, async (req, res) => {
    try {
        const deviceId = req.params.id;
        const { folderId } = req.body;
        
        // Use serverBackend to verify device exists (works in both modes)
        const device = await serverBackend.getDeviceById(deviceId);
        if (!device) {
            return res.status(404).json({
                success: false,
                error: req.t('devices.not_found')
            });
        }
        
        // Verify folder exists (null to unassign)
        if (folderId !== null && folderId !== undefined) {
            const folder = await db.getFolderById(folderId);
            if (!folder) {
                return res.status(404).json({
                    success: false,
                    error: req.t('folders.not_found')
                });
            }
        }
        
        await db.assignDeviceToFolder(deviceId, folderId);
        
        res.json({ success: true });
    } catch (err) {
        console.error('Assign device folder error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

module.exports = router;

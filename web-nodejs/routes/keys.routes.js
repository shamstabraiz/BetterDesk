/**
 * Yomie Console - Keys Routes
 */

const express = require('express');
const router = express.Router();
const keyService = require('../services/keyService');
const { requireAuth } = require('../middleware/auth');

/**
 * GET /keys - Keys management page
 */
router.get('/keys', requireAuth, (req, res) => {
    res.render('keys', {
        title: req.t('nav.keys'),
        activePage: 'keys'
    });
});

/**
 * GET /api/keys/public - Get public key
 */
router.get('/api/keys/public', requireAuth, (req, res) => {
    try {
        const publicKey = keyService.getPublicKey();
        
        if (!publicKey) {
            return res.status(404).json({
                success: false,
                error: req.t('keys.not_found')
            });
        }
        
        res.json({
            success: true,
            data: {
                key: publicKey
            }
        });
    } catch (err) {
        console.error('Get public key error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * GET /api/keys/public/qr - Get server config as QR code
 * Generates a QR code in rustdesk://config/<base64-json> format
 * that the RustDesk mobile app can scan to auto-configure.
 */
router.get('/api/keys/public/qr', requireAuth, async (req, res) => {
    try {
        // Determine the server host from request headers (what the browser used to reach us)
        let serverHost = req.headers['x-forwarded-host'] || req.headers.host || req.hostname || 'localhost';
        serverHost = serverHost.split(':')[0]; // strip port

        const qrDataUrl = await keyService.getServerConfigQR(serverHost);
        
        if (!qrDataUrl) {
            return res.status(404).json({
                success: false,
                error: req.t('keys.not_found')
            });
        }
        
        res.json({
            success: true,
            data: {
                qr: qrDataUrl
            }
        });
    } catch (err) {
        console.error('Get public key QR error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * GET /api/keys/public/download - Download public key file
 */
router.get('/api/keys/public/download', requireAuth, (req, res) => {
    try {
        const publicKey = keyService.getPublicKey();
        
        if (!publicKey) {
            return res.status(404).json({
                success: false,
                error: req.t('keys.not_found')
            });
        }
        
        res.setHeader('Content-Type', 'text/plain');
        res.setHeader('Content-Disposition', 'attachment; filename="id_ed25519.pub"');
        res.send(publicKey);
    } catch (err) {
        console.error('Download public key error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * GET /api/keys/api - Get API key (masked)
 */
router.get('/api/keys/api', requireAuth, (req, res) => {
    try {
        const show = req.query.show === 'true';
        const apiKey = keyService.getApiKey(!show);
        
        if (!apiKey) {
            return res.status(404).json({
                success: false,
                error: req.t('keys.api_not_found')
            });
        }
        
        res.json({
            success: true,
            data: {
                key: apiKey,
                masked: !show
            }
        });
    } catch (err) {
        console.error('Get API key error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * GET /api/keys/server-info - Get server address info for config display
 */
router.get('/api/keys/server-info', requireAuth, (req, res) => {
    try {
        const apiKey = keyService.getApiKey(true);
        
        // Get server IP - prefer X-Forwarded-Host, then Host header, then hostname
        let serverIp = req.headers['x-forwarded-host'] || req.headers.host || req.hostname || '-';
        // Remove port if present
        serverIp = serverIp.split(':')[0];
        
        res.json({
            success: true,
            data: {
                server_id: serverIp,
                relay_server: serverIp,
                api_key_masked: apiKey || '••••••••'
            }
        });
    } catch (err) {
        console.error('Get server info error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * GET /api/keys/qr - Alias for /api/keys/public/qr (backward compatibility)
 */
router.get('/api/keys/qr', requireAuth, async (req, res) => {
    try {
        let serverHost = req.headers['x-forwarded-host'] || req.headers.host || req.hostname || 'localhost';
        serverHost = serverHost.split(':')[0];

        const qrDataUrl = await keyService.getServerConfigQR(serverHost);
        
        if (!qrDataUrl) {
            return res.status(404).json({
                success: false,
                error: req.t('keys.not_found')
            });
        }
        
        res.json({
            success: true,
            data: {
                qr: qrDataUrl
            }
        });
    } catch (err) {
        console.error('Get public key QR error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

module.exports = router;

/**
 * Yomie Console - Client Generator Routes
 */

const express = require('express');
const router = express.Router();
const { requireAuth } = require('../middleware/auth');
const keyService = require('../services/keyService');
const config = require('../config/config');

/**
 * GET /generator - Client generator page
 */
router.get('/generator', requireAuth, (req, res) => {
    res.render('generator', {
        title: req.t('nav.generator'),
        activePage: 'generator'
    });
});

/**
 * GET /api/generator/config - Get generator configuration
 */
router.get('/api/generator/config', requireAuth, (req, res) => {
    try {
        const publicKey = keyService.getPublicKey();
        
        res.json({
            success: true,
            data: {
                publicKey: publicKey,
                serverUrl: config.hbbsApiUrl.replace('/api', ''),
                // Default client config values
                defaults: {
                    rendezvousServer: '',
                    apiServer: '',
                    key: publicKey || ''
                }
            }
        });
    } catch (err) {
        console.error('Get generator config error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * POST /api/generator/generate-config - Generate client configuration
 */
router.post('/api/generator/generate-config', requireAuth, (req, res) => {
    try {
        const { serverHost, serverPort, relayHost, relayPort, clientName } = req.body;
        
        if (!serverHost) {
            return res.status(400).json({
                success: false,
                error: 'Server host is required'
            });
        }
        
        const publicKey = keyService.getPublicKey();
        
        // Generate configuration string for RustDesk client
        const configLines = [];
        
        if (serverHost) {
            configLines.push(`rendezvous_server = ${serverHost}:${serverPort || 21116}`);
        }
        
        if (relayHost) {
            configLines.push(`relay_server = ${relayHost}:${relayPort || 21117}`);
        }
        
        if (publicKey) {
            configLines.push(`key = ${publicKey}`);
        }
        
        if (clientName) {
            configLines.push(`name = ${clientName}`);
        }
        
        const configText = configLines.join('\n');
        
        res.json({
            success: true,
            data: {
                config: configText,
                fileName: 'rustdesk.toml'
            }
        });
    } catch (err) {
        console.error('Generate config error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

module.exports = router;

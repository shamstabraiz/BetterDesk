/**
 * Yomie Console - Generator Page
 */

(function() {
    'use strict';
    
    document.addEventListener('DOMContentLoaded', init);
    
    let publicKey = '';
    
    function init() {
        loadPublicKey();
        initForm();
        
        // Copy config button
        document.getElementById('copy-config-btn')?.addEventListener('click', copyConfig);
    }
    
    /**
     * Load public key for config generation
     */
    async function loadPublicKey() {
        try {
            const data = await Utils.api('/api/keys/public');
            publicKey = data.key || '';
        } catch (error) {
            console.error('Failed to load public key:', error);
        }
    }
    
    /**
     * Initialize form
     */
    function initForm() {
        const form = document.getElementById('generator-form');
        if (!form) return;
        
        form.addEventListener('submit', (e) => {
            e.preventDefault();
            generateConfig();
        });
    }
    
    /**
     * Generate configuration
     */
    function generateConfig() {
        const serverAddress = document.getElementById('server-address').value.trim();
        const customPort = document.getElementById('custom-port').value.trim();
        const relayServer = document.getElementById('relay-server').value.trim();
        const includeKey = document.getElementById('include-key').checked;
        
        if (!serverAddress) {
            Notifications.error(_('generator.server_required'));
            return;
        }
        
        // Build server string
        let idServer = serverAddress;
        if (customPort && customPort !== '21116') {
            idServer += `:${customPort}`;
        }
        
        // Relay server (same as ID server if not specified)
        const relay = relayServer || serverAddress;
        
        // Build config object
        const config = {
            id_server: idServer,
            relay_server: relay
        };
        
        if (includeKey && publicKey) {
            config.key = publicKey;
        }
        
        // Format output
        const configJson = JSON.stringify(config, null, 2);
        
        // Show output
        const outputSection = document.getElementById('generator-output');
        const configOutput = document.getElementById('config-output');
        
        configOutput.textContent = configJson;
        outputSection.classList.remove('hidden');
        
        // Update info fields
        document.getElementById('output-server').textContent = idServer;
        document.getElementById('output-relay').textContent = relay;
        document.getElementById('output-key').textContent = publicKey ? publicKey.substring(0, 20) + '...' : '-';
        
        Notifications.success(_('generator.generated'));
    }
    
    /**
     * Copy config to clipboard
     */
    async function copyConfig() {
        const configOutput = document.getElementById('config-output');
        if (!configOutput) return;
        
        const config = configOutput.textContent;
        if (!config) {
            Notifications.warning(_('generator.nothing_to_copy'));
            return;
        }
        
        await Utils.copyToClipboard(config);
        Notifications.success(_('common.copied'));
    }
    
})();

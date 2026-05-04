/**
 * Yomie Console - Client-side i18n
 */

(function() {
    'use strict';
    
    const translations = window.Yomie?.translations || {};
    const currentLang = window.Yomie?.lang || 'en';
    
    /**
     * Get translation by key with optional interpolation
     * @param {string} key - Translation key (e.g., 'nav.dashboard')
     * @param {Object} params - Parameters for interpolation
     * @returns {string} Translated string or key if not found
     */
    function translate(key, params = {}) {
        // Get nested value by dot notation
        const keys = key.split('.');
        let value = translations;
        
        for (const k of keys) {
            if (value && typeof value === 'object' && k in value) {
                value = value[k];
            } else {
                // Key not found, return the key itself
                return key;
            }
        }
        
        if (typeof value !== 'string') {
            return key;
        }
        
        // Interpolate parameters: {param} → value
        return value.replace(/\{(\w+)\}/g, (match, param) => {
            return params[param] !== undefined ? params[param] : match;
        });
    }
    
    /**
     * Alias for translate
     */
    window._ = translate;
    window.t = translate;
    
    /**
     * Get current language code
     */
    window.getCurrentLang = function() {
        return currentLang;
    };
    
    /**
     * Change language by reloading with new lang parameter
     */
    window.changeLanguage = async function(langCode) {
        try {
            // Set language preference via API
            await Utils.api(`/api/i18n/set/${langCode}`, { method: 'POST' });
            
            // Reload the page to apply
            window.location.reload();
        } catch (error) {
            console.error('Failed to change language:', error);
            Notifications.error(_('errors.language_change_failed'));
        }
    };
    
    /**
     * Pluralization helper
     * @param {number} count - Number to check
     * @param {Object} forms - { one: '1 item', other: '{count} items' }
     */
    window.plural = function(count, forms) {
        const form = count === 1 ? 'one' : 'other';
        const template = forms[form] || forms.other || '';
        return template.replace('{count}', count);
    };
    
})();

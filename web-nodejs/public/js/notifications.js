/**
 * Yomie Console - Toast Notifications
 */

const Notifications = (function() {
    'use strict';
    
    let container = null;
    
    function getContainer() {
        if (!container) {
            container = document.getElementById('toast-container');
            if (!container) {
                container = document.createElement('div');
                container.id = 'toast-container';
                container.className = 'toast-container';
                document.body.appendChild(container);
            }
        }
        return container;
    }
    
    function getIcon(type) {
        switch (type) {
            case 'success': return 'check_circle';
            case 'error': return 'error';
            case 'warning': return 'warning';
            case 'info': default: return 'info';
        }
    }
    
    /**
     * Show a toast notification
     * @param {Object} options
     * @param {string} options.type - 'success', 'error', 'warning', 'info'
     * @param {string} options.title - Toast title
     * @param {string} options.message - Toast message
     * @param {number} options.duration - Auto-dismiss duration in ms (0 for no auto-dismiss)
     */
    function show({ type = 'info', title = '', message = '', duration = 5000 }) {
        const container = getContainer();
        const id = Utils.generateId();
        
        const toast = document.createElement('div');
        toast.id = id;
        toast.className = `toast ${type}`;
        toast.innerHTML = `
            <span class="material-icons toast-icon">${getIcon(type)}</span>
            <div class="toast-content">
                ${title ? `<div class="toast-title">${Utils.escapeHtml(title)}</div>` : ''}
                ${message ? `<div class="toast-message">${Utils.escapeHtml(message)}</div>` : ''}
            </div>
            <button class="toast-close" aria-label="Close">
                <span class="material-icons">close</span>
            </button>
        `;
        
        // Close button handler
        toast.querySelector('.toast-close').addEventListener('click', () => {
            dismiss(id);
        });
        
        container.appendChild(toast);
        
        // Auto-dismiss
        if (duration > 0) {
            setTimeout(() => dismiss(id), duration);
        }
        
        return id;
    }
    
    /**
     * Dismiss a toast by ID
     */
    function dismiss(id) {
        const toast = document.getElementById(id);
        if (toast) {
            toast.style.animation = 'slideOut 0.3s ease forwards';
            setTimeout(() => toast.remove(), 300);
        }
    }
    
    /**
     * Dismiss all toasts
     */
    function dismissAll() {
        const container = getContainer();
        container.innerHTML = '';
    }
    
    // Convenience methods
    function success(message, title = '') {
        return show({ type: 'success', title, message });
    }
    
    function error(message, title = '') {
        return show({ type: 'error', title, message, duration: 8000 });
    }
    
    function warning(message, title = '') {
        return show({ type: 'warning', title, message });
    }
    
    function info(message, title = '') {
        return show({ type: 'info', title, message });
    }
    
    // Add slide out animation
    const style = document.createElement('style');
    style.textContent = `
        @keyframes slideOut {
            from {
                transform: translateX(0);
                opacity: 1;
            }
            to {
                transform: translateX(100%);
                opacity: 0;
            }
        }
    `;
    document.head.appendChild(style);
    
    return {
        show,
        dismiss,
        dismissAll,
        success,
        error,
        warning,
        info
    };
})();

// Export
window.Notifications = Notifications;

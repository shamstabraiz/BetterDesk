/**
 * Yomie Console - CDAP Command Sender
 * Handles sending commands to CDAP devices with confirmation dialogs,
 * cooldown management, and command log tracking.
 */

(function () {
    'use strict';

    const COOLDOWN_MS = 1000;
    const MAX_LOG_ENTRIES = 50;

    const lastCommandTime = {};
    const commandLog = [];

    // ── Command Sending ──────────────────────────────────────────────────

    async function send(deviceId, widgetId, action, value, reason) {
        // Cooldown check per widget
        const key = `${deviceId}:${widgetId}`;
        const now = Date.now();
        if (lastCommandTime[key] && (now - lastCommandTime[key]) < COOLDOWN_MS) {
            return;
        }
        lastCommandTime[key] = now;

        const csrfToken = window.Yomie?.csrfToken || '';

        try {
            const res = await fetch(`/api/cdap/devices/${encodeURIComponent(deviceId)}/command`, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                    'X-CSRF-Token': csrfToken
                },
                body: JSON.stringify({
                    widget_id: widgetId,
                    action: action,
                    value: value,
                    reason: reason || undefined
                })
            });

            const data = await res.json();
            const success = res.ok && data.success;

            addLogEntry({
                time: new Date(),
                widgetId,
                action,
                value,
                success,
                error: success ? null : (data.error || 'Unknown error')
            });

            if (!success) {
                showToast(data.error || 'Command failed', 'error');
            }

            return data;
        } catch (err) {
            addLogEntry({
                time: new Date(),
                widgetId,
                action,
                value,
                success: false,
                error: err.message
            });
            showToast('Failed to send command', 'error');
            return null;
        }
    }

    function sendWithConfirm(deviceId, widgetId, action, value, confirmMsg) {
        const __ = window.Yomie?.translations || {};
        const title = __?.cdap?.confirm_command || 'Confirm Command';

        if (!window.YomieModal) {
            if (confirm(confirmMsg || title)) {
                return send(deviceId, widgetId, action, value);
            }
            return Promise.resolve(null);
        }

        return new Promise((resolve) => {
            window.YomieModal.confirm({
                title: title,
                message: confirmMsg || `${action} → ${widgetId}?`,
                confirmText: __?.common?.confirm || 'Confirm',
                cancelText: __?.common?.cancel || 'Cancel',
                type: 'warning',
                onConfirm: async () => {
                    const result = await send(deviceId, widgetId, action, value);
                    resolve(result);
                },
                onCancel: () => resolve(null)
            });
        });
    }

    // ── Command Log ──────────────────────────────────────────────────────

    function addLogEntry(entry) {
        commandLog.unshift(entry);
        if (commandLog.length > MAX_LOG_ENTRIES) commandLog.pop();
        renderLog();
    }

    function renderLog() {
        const container = document.getElementById('cdap-log-entries');
        if (!container) return;

        let html = '';
        for (const entry of commandLog) {
            const time = entry.time.toLocaleTimeString();
            const icon = entry.success ? 'check_circle' : 'error';
            const cls = entry.success ? 'cdap-log-success' : 'cdap-log-error';
            const valueStr = entry.value !== null && entry.value !== undefined
                ? ` = ${escapeHtml(String(entry.value))}`
                : '';
            const errorStr = entry.error
                ? `<span class="cdap-log-error-msg">${escapeHtml(entry.error)}</span>`
                : '';

            html += `
                <div class="cdap-log-entry ${cls}">
                    <span class="material-icons cdap-log-icon">${icon}</span>
                    <span class="cdap-log-time">${time}</span>
                    <span class="cdap-log-detail">
                        <strong>${escapeHtml(entry.widgetId)}</strong>
                        → ${escapeHtml(entry.action)}${valueStr}
                    </span>
                    ${errorStr}
                </div>
            `;
        }

        container.innerHTML = html || '<div class="cdap-log-empty">No commands sent yet</div>';
    }

    function clearLog() {
        commandLog.length = 0;
        renderLog();
    }

    // ── Init ─────────────────────────────────────────────────────────────

    function init() {
        const clearBtn = document.getElementById('cdap-clear-log');
        if (clearBtn) {
            clearBtn.addEventListener('click', clearLog);
        }
        renderLog();
    }

    // ── Utilities ────────────────────────────────────────────────────────

    function escapeHtml(str) {
        if (!str) return '';
        const div = document.createElement('div');
        div.textContent = String(str);
        return div.innerHTML;
    }

    function showToast(message, type) {
        if (window.YomieNotifications?.show) {
            window.YomieNotifications.show(message, type);
        }
    }

    // ── Public API ───────────────────────────────────────────────────────

    window.CDAPCommands = {
        send,
        sendWithConfirm,
        clearLog,
        getLog: () => [...commandLog]
    };

    // Auto-init
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();

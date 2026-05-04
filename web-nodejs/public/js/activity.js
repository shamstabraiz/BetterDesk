/**
 * Yomie Console - Activity Page Script
 */

(function () {
    'use strict';

    const _ = window.Yomie?.translations
        ? (key) => {
            const keys = key.split('.');
            let val = window.Yomie.translations;
            for (const k of keys) { val = val?.[k]; }
            return val || key;
        }
        : (key) => key;

    const csrfToken = window.Yomie?.csrfToken || '';

    async function apiFetch(url, options = {}) {
        const headers = { 'Content-Type': 'application/json' };
        if (csrfToken) headers['x-csrf-token'] = csrfToken;
        const resp = await fetch(url, { ...options, headers: { ...headers, ...options.headers } });
        return resp.json();
    }

    function escapeHtml(str) {
        if (!str) return '';
        const div = document.createElement('div');
        div.textContent = String(str);
        return div.innerHTML;
    }

    function formatTimeAgo(dateStr) {
        if (!dateStr) return '—';
        const d = new Date(dateStr);
        const diff = Math.floor((Date.now() - d) / 1000);
        if (diff < 60) return _('time.seconds_ago').replace('{count}', diff);
        if (diff < 3600) return _('time.minutes_ago').replace('{count}', Math.floor(diff / 60));
        if (diff < 86400) return _('time.hours_ago').replace('{count}', Math.floor(diff / 3600));
        if (diff < 2592000) return _('time.days_ago').replace('{count}', Math.floor(diff / 86400));
        return d.toLocaleDateString();
    }

    function formatDuration(seconds) {
        if (!seconds || seconds < 0) return '—';
        const h = Math.floor(seconds / 3600);
        const m = Math.floor((seconds % 3600) / 60);
        const s = seconds % 60;
        if (h > 0) return `${h}h ${m}m`;
        if (m > 0) return `${m}m ${s}s`;
        return `${s}s`;
    }

    function showToast(message, type) {
        if (window.Yomie?.notify) { window.Yomie.notify(message, type); return; }
        const container = document.getElementById('toast-container');
        if (!container) return;
        const toast = document.createElement('div');
        toast.className = `toast toast-${type || 'info'}`;
        toast.textContent = message;
        container.appendChild(toast);
        setTimeout(() => toast.remove(), 4000);
    }

    // ---- Modal close ----
    document.querySelectorAll('.modal-close').forEach(btn => {
        btn.addEventListener('click', () => {
            const id = btn.dataset.modal;
            if (id) document.getElementById(id).style.display = 'none';
        });
    });

    // ---- Elements ----
    const activityBody = document.getElementById('activity-body');
    const searchInput = document.getElementById('activity-search');
    const dateFrom = document.getElementById('activity-from');
    const dateTo = document.getElementById('activity-to');
    const refreshBtn = document.getElementById('activity-refresh-btn');

    let allSessions = [];

    // ---- Load ----
    async function loadActivity() {
        activityBody.innerHTML = `<tr class="loading-row"><td colspan="7">${_('common.loading')}</td></tr>`;
        const params = new URLSearchParams();
        if (dateFrom.value) params.set('from', dateFrom.value);
        if (dateTo.value) params.set('to', dateTo.value);

        try {
            const data = await apiFetch(`/api/activity?${params}`);
            allSessions = Array.isArray(data) ? data : (data.summaries || []);
            filterAndRender();
        } catch (err) {
            activityBody.innerHTML = `<tr class="empty-row"><td colspan="7">
                <div class="empty-state"><span class="material-icons">error_outline</span><p>${escapeHtml(err.message)}</p></div></td></tr>`;
        }
    }

    function filterAndRender() {
        const q = searchInput.value.toLowerCase();
        const filtered = q
            ? allSessions.filter(s => ((s.device_id || '') + (s.hostname || '')).toLowerCase().includes(q))
            : allSessions;
        renderActivity(filtered);
    }

    function renderActivity(sessions) {
        if (!sessions.length) {
            activityBody.innerHTML = `<tr class="empty-row"><td colspan="7">
                <div class="empty-state"><span class="material-icons">history</span><p>${_('activity.no_activity')}</p></div></td></tr>`;
            return;
        }
        activityBody.innerHTML = sessions.map(s => `
            <tr>
                <td>${escapeHtml(s.device_id)}</td>
                <td>${escapeHtml(s.hostname || '—')}</td>
                <td>${s.session_count || 0}</td>
                <td class="duration-cell">${formatDuration(s.total_active_secs)}</td>
                <td class="duration-cell">${formatDuration(s.idle_seconds)}</td>
                <td class="time-cell">${formatTimeAgo(s.reported_at)}</td>
                <td><button class="btn btn-sm btn-secondary detail-btn" data-id="${escapeHtml(s.device_id)}"><span class="material-icons" style="font-size:16px">visibility</span></button></td>
            </tr>
        `).join('');

        activityBody.querySelectorAll('.detail-btn').forEach(btn => {
            btn.addEventListener('click', () => showDetail(btn.dataset.id));
        });
    }

    async function showDetail(deviceId) {
        const modal = document.getElementById('detail-modal');
        const sessionsList = document.getElementById('sessions-list');
        const appsList = document.getElementById('apps-list');
        modal.style.display = 'flex';
        document.getElementById('detail-modal-title').textContent = `${_('activity.detail')} — ${deviceId}`;
        sessionsList.innerHTML = `<p>${_('common.loading')}</p>`;
        appsList.innerHTML = `<p>${_('common.loading')}</p>`;

        try {
            const [sessions, top] = await Promise.all([
                apiFetch(`/api/activity/${deviceId}`),
                apiFetch(`/api/activity/${deviceId}/top`)
            ]);

            // Sessions
            const sesList = Array.isArray(sessions) ? sessions : (sessions.sessions || []);
            if (!sesList.length) {
                sessionsList.innerHTML = `<p class="text-muted">${_('activity.no_sessions')}</p>`;
            } else {
                sessionsList.innerHTML = sesList.map(s => `
                    <div class="session-item">
                        <span class="session-time">${formatTimeAgo(s.session_start)}</span>
                        <span class="session-user">${escapeHtml(s.user || '—')}</span>
                        <span class="session-dur">${formatDuration(s.duration_seconds)}</span>
                    </div>
                `).join('');
            }

            // Top apps
            const appList = Array.isArray(top) ? top : (top.top_apps || []);
            if (!appList.length) {
                appsList.innerHTML = `<p class="text-muted">${_('activity.no_apps')}</p>`;
            } else {
                const maxTime = Math.max(...appList.map(a => a.total_seconds || 0), 1);
                appsList.innerHTML = appList.map(a => {
                    const pct = Math.round(((a.total_seconds || 0) / maxTime) * 100);
                    return `
                    <div class="app-item">
                        <span class="app-name">${escapeHtml(a.name)}</span>
                        <div class="app-bar-wrapper">
                            <div class="app-bar"><div class="app-bar-fill" style="width:${pct}%"></div></div>
                            <span class="app-time">${formatDuration(a.total_seconds)}</span>
                        </div>
                    </div>`;
                }).join('');
            }
        } catch (err) {
            sessionsList.innerHTML = `<p class="text-error">${escapeHtml(err.message)}</p>`;
            appsList.innerHTML = '';
        }
    }

    // ---- Events ----
    searchInput.addEventListener('input', filterAndRender);
    dateFrom.addEventListener('change', loadActivity);
    dateTo.addEventListener('change', loadActivity);
    refreshBtn.addEventListener('click', loadActivity);

    // ---- Init ----
    loadActivity();
})();

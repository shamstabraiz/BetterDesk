/**
 * Yomie Console - Network Monitor Script
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

    // ---- Tab switching ----
    document.querySelectorAll('.tab-btn').forEach(btn => {
        btn.addEventListener('click', () => {
            document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
            document.querySelectorAll('.tab-panel').forEach(p => p.classList.remove('active'));
            btn.classList.add('active');
            const panel = document.getElementById('tab-' + btn.dataset.tab);
            if (panel) panel.classList.add('active');
        });
    });

    // ---- Modal close ----
    document.querySelectorAll('.modal-close').forEach(btn => {
        btn.addEventListener('click', () => {
            const id = btn.dataset.modal;
            if (id) document.getElementById(id).style.display = 'none';
        });
    });

    // ---- Searching ----
    let allTargets = [];
    const searchInput = document.getElementById('target-search');
    searchInput.addEventListener('input', () => {
        const q = searchInput.value.toLowerCase();
        const filtered = q ? allTargets.filter(t => (t.name + t.host).toLowerCase().includes(q)) : allTargets;
        renderTargets(filtered);
    });

    // ==== TARGETS ====

    const targetsBody = document.getElementById('targets-body');
    const createBtn = document.getElementById('create-target-btn');
    const targetModal = document.getElementById('target-modal');
    const saveBtn = document.getElementById('target-save-btn');
    const cancelBtn = document.getElementById('target-cancel-btn');

    async function loadTargets() {
        targetsBody.innerHTML = `<tr class="loading-row"><td colspan="8">${_('common.loading')}</td></tr>`;
        try {
            const data = await apiFetch('/api/network/targets');
            allTargets = Array.isArray(data) ? data : [];
            renderTargets(allTargets);
        } catch (err) {
            targetsBody.innerHTML = `<tr class="empty-row"><td colspan="8">
                <div class="empty-state"><span class="material-icons">error_outline</span><p>${escapeHtml(err.message)}</p></div></td></tr>`;
        }
    }

    async function loadStats() {
        try {
            const stats = await apiFetch('/api/network/stats');
            document.getElementById('stat-total').textContent = stats.total ?? 0;
            document.getElementById('stat-up').textContent = stats.up ?? 0;
            document.getElementById('stat-down').textContent = stats.down ?? 0;
            document.getElementById('stat-avg-latency').textContent = stats.avg_latency != null ? Math.round(stats.avg_latency) + ' ms' : '—';
        } catch (_e) { /* ignore */ }
    }

    function renderTargets(targets) {
        if (!targets.length) {
            targetsBody.innerHTML = `<tr class="empty-row"><td colspan="8">
                <div class="empty-state"><span class="material-icons">dns</span><p>${_('network.no_targets')}</p></div></td></tr>`;
            return;
        }
        targetsBody.innerHTML = targets.map(t => {
            const status = t.last_status || 'unknown';
            const latency = t.last_latency_ms;
            const latencyClass = latency == null ? '' : latency < 100 ? 'good' : latency < 500 ? 'warn' : 'bad';
            return `
            <tr data-id="${t.id}">
                <td>${escapeHtml(t.name)}</td>
                <td>${escapeHtml(t.host)}</td>
                <td>${escapeHtml(t.check_type || 'ping')}</td>
                <td>${t.port || '—'}</td>
                <td>${t.interval_ms ? Math.round(t.interval_ms / 1000) : (t.check_interval || 60)}s</td>
                <td><span class="target-status ${status}"><span class="dot"></span> ${status.toUpperCase()}</span></td>
                <td><span class="latency-val ${latencyClass}">${latency != null ? latency + ' ms' : '—'}</span></td>
                <td class="action-btn-group">
                    <button class="btn btn-sm btn-secondary check-btn" data-id="${t.id}" title="${_('network.check_now')}"><span class="material-icons" style="font-size:16px">refresh</span></button>
                    <button class="btn btn-sm btn-secondary hist-btn" data-id="${t.id}" title="${_('network.check_history')}"><span class="material-icons" style="font-size:16px">history</span></button>
                    <button class="btn btn-sm btn-secondary edit-btn" data-id="${t.id}"><span class="material-icons" style="font-size:16px">edit</span></button>
                    <button class="btn btn-sm btn-danger del-btn" data-id="${t.id}"><span class="material-icons" style="font-size:16px">delete</span></button>
                </td>
            </tr>`;
        }).join('');

        targetsBody.querySelectorAll('.check-btn').forEach(btn => btn.addEventListener('click', () => checkTarget(btn.dataset.id)));
        targetsBody.querySelectorAll('.hist-btn').forEach(btn => btn.addEventListener('click', () => showHistory(btn.dataset.id)));
        targetsBody.querySelectorAll('.edit-btn').forEach(btn => btn.addEventListener('click', () => editTarget(btn.dataset.id)));
        targetsBody.querySelectorAll('.del-btn').forEach(btn => btn.addEventListener('click', () => deleteTarget(btn.dataset.id)));
    }

    function openTargetModal(t) {
        document.getElementById('target-modal-title').textContent = t ? _('network.edit_target') : _('network.add_target');
        document.getElementById('target-id').value = t ? t.id : '';
        document.getElementById('target-name').value = t ? t.name : '';
        document.getElementById('target-host').value = t ? t.host : '';
        document.getElementById('target-type').value = t ? t.check_type : 'ping';
        document.getElementById('target-port').value = t ? t.port || 80 : 80;
        document.getElementById('target-interval').value = t ? String(t.interval_ms ? Math.round(t.interval_ms / 1000) : (t.check_interval || 60)) : '60';
        document.getElementById('target-enabled').checked = t ? !!t.enabled : true;
        targetModal.style.display = 'flex';
    }

    async function editTarget(id) {
        try {
            const t = await apiFetch(`/api/network/targets/${id}`);
            openTargetModal(t);
        } catch (err) { showToast(err.message, 'error'); }
    }

    async function saveTarget() {
        const id = document.getElementById('target-id').value;
        const body = {
            name: document.getElementById('target-name').value.trim(),
            host: document.getElementById('target-host').value.trim(),
            check_type: document.getElementById('target-type').value,
            port: parseInt(document.getElementById('target-port').value, 10) || null,
            check_interval: parseInt(document.getElementById('target-interval').value, 10) || 60,
            interval_ms: (parseInt(document.getElementById('target-interval').value, 10) || 60) * 1000,
            enabled: document.getElementById('target-enabled').checked,
        };
        if (!body.name || !body.host) { showToast(_('network.name_host_required'), 'error'); return; }
        try {
            const url = id ? `/api/network/targets/${id}` : '/api/network/targets';
            const method = id ? 'PATCH' : 'POST';
            await apiFetch(url, { method, body: JSON.stringify(body) });
            targetModal.style.display = 'none';
            showToast(_('common.saved'), 'success');
            loadTargets();
            loadStats();
        } catch (err) { showToast(err.message, 'error'); }
    }

    async function deleteTarget(id) {
        if (!confirm(_('network.delete_target') + '?')) return;
        try {
            await apiFetch(`/api/network/targets/${id}`, { method: 'DELETE' });
            showToast(_('common.success'), 'success');
            loadTargets();
            loadStats();
        } catch (err) { showToast(err.message, 'error'); }
    }

    async function checkTarget(id) {
        try {
            const result = await apiFetch(`/api/network/targets/${id}/check`, { method: 'POST' });
            showToast(`${result.status === 'up' ? '✓' : '✗'} ${result.latency_ms != null ? result.latency_ms + 'ms' : ''}`, result.status === 'up' ? 'success' : 'error');
            loadTargets();
            loadStats();
        } catch (err) { showToast(err.message, 'error'); }
    }

    createBtn?.addEventListener('click', () => openTargetModal(null));
    saveBtn?.addEventListener('click', saveTarget);
    cancelBtn?.addEventListener('click', () => { targetModal.style.display = 'none'; });

    async function showHistory(id) {
        const modal = document.getElementById('history-modal');
        const body = document.getElementById('history-body');
        modal.style.display = 'flex';
        body.innerHTML = `<p>${_('common.loading')}</p>`;
        try {
            const history = await apiFetch(`/api/network/targets/${id}/history`);
            const entries = Array.isArray(history) ? history : [];
            if (!entries.length) {
                body.innerHTML = `<div class="empty-state"><span class="material-icons">history</span><p>${_('network.no_history')}</p></div>`;
                return;
            }
            body.innerHTML = `<div class="history-timeline">${entries.map(e => `
                <div class="history-entry">
                    <span class="time">${formatTimeAgo(e.checked_at)}</span>
                    <span class="status-dot ${e.status}"></span>
                    <span>${e.status.toUpperCase()}</span>
                    <span class="latency">${e.latency_ms != null ? e.latency_ms + ' ms' : '—'}</span>
                </div>`).join('')}</div>`;
        } catch (err) {
            body.innerHTML = `<p class="text-error">${escapeHtml(err.message)}</p>`;
        }
    }

    createBtn.addEventListener('click', () => openTargetModal(null));
    saveBtn.addEventListener('click', saveTarget);

    // ==== TOOLS ====

    async function runTool(endpoint, body, resultEl) {
        resultEl.textContent = _('common.loading');
        resultEl.className = 'tool-result';
        try {
            const result = await apiFetch(endpoint, { method: 'POST', body: JSON.stringify(body) });
            resultEl.className = 'tool-result ' + (result.success ? 'success' : 'error');
            resultEl.textContent = JSON.stringify(result, null, 2);
        } catch (err) {
            resultEl.className = 'tool-result error';
            resultEl.textContent = err.message;
        }
    }

    document.getElementById('tool-ping-btn').addEventListener('click', () => {
        runTool('/api/network/ping', { host: document.getElementById('tool-ping-host').value.trim() }, document.getElementById('tool-ping-result'));
    });

    document.getElementById('tool-tcp-btn').addEventListener('click', () => {
        runTool('/api/network/tcp', {
            host: document.getElementById('tool-tcp-host').value.trim(),
            port: parseInt(document.getElementById('tool-tcp-port').value, 10) || 80
        }, document.getElementById('tool-tcp-result'));
    });

    document.getElementById('tool-http-btn').addEventListener('click', () => {
        runTool('/api/network/http', { url: document.getElementById('tool-http-url').value.trim() }, document.getElementById('tool-http-result'));
    });

    document.getElementById('tool-dns-btn').addEventListener('click', () => {
        runTool('/api/network/resolve', { host: document.getElementById('tool-dns-host').value.trim() }, document.getElementById('tool-dns-result'));
    });

    // ==== Monitor toggle ====

    const monitorBtn = document.getElementById('monitor-toggle-btn');
    let monitorRunning = false;

    async function checkMonitorStatus() {
        try {
            const data = await apiFetch('/api/network/monitor/status');
            monitorRunning = !!data.running;
            updateMonitorBtn();
        } catch (_e) { /* ignore */ }
    }

    function updateMonitorBtn() {
        if (monitorRunning) {
            monitorBtn.innerHTML = `<span class="material-icons">stop</span> ${_('network.monitor_stop')}`;
            monitorBtn.classList.add('monitor-active');
        } else {
            monitorBtn.innerHTML = `<span class="material-icons">play_arrow</span> ${_('network.monitor_start')}`;
            monitorBtn.classList.remove('monitor-active');
        }
    }

    monitorBtn.addEventListener('click', async () => {
        const action = monitorRunning ? 'stop' : 'start';
        try {
            await apiFetch(`/api/network/monitor/${action}`, { method: 'POST' });
            monitorRunning = !monitorRunning;
            updateMonitorBtn();
            showToast(_('common.success'), 'success');
        } catch (err) { showToast(err.message, 'error'); }
    });

    // ---- Init ----
    loadTargets();
    loadStats();
    checkMonitorStatus();
})();

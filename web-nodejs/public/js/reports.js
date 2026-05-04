/**
 * Yomie Console - Reports Page Script
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
        if (!resp.ok) {
            const text = await resp.text();
            let msg;
            try { msg = JSON.parse(text).error; } catch (_e) { msg = resp.statusText; }
            throw new Error(msg || `HTTP ${resp.status}`);
        }
        return resp;
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
            if (btn.dataset.tab === 'saved') loadSaved();
        });
    });

    // ==== Generate ====

    let selectedType = null;
    let lastReportData = null;
    const optionsPanel = document.getElementById('report-options');
    const optionsTitle = document.getElementById('report-options-title');
    const previewPanel = document.getElementById('report-preview');
    const previewContent = document.getElementById('preview-content');

    document.querySelectorAll('.report-type-card').forEach(card => {
        card.addEventListener('click', () => {
            document.querySelectorAll('.report-type-card').forEach(c => c.classList.remove('selected'));
            card.classList.add('selected');
            selectedType = card.dataset.type;
            optionsTitle.textContent = card.querySelector('h3').textContent;
            optionsPanel.style.display = '';
            previewPanel.style.display = 'none';
        });
    });

    document.getElementById('generate-btn').addEventListener('click', async () => {
        if (!selectedType) return;
        const format = document.getElementById('report-format').value;
        const body = {
            type: selectedType,
            from: document.getElementById('report-from').value || undefined,
            to: document.getElementById('report-to').value || undefined,
        };

        try {
            const endpoint = format === 'csv' ? '/api/reports/generate/csv' : '/api/reports/generate';
            const resp = await apiFetch(endpoint, { method: 'POST', body: JSON.stringify(body) });

            if (format === 'csv') {
                const text = await resp.text();
                lastReportData = text;
                previewContent.textContent = text;
            } else {
                const json = await resp.json();
                lastReportData = JSON.stringify(json, null, 2);
                renderReport(json);
            }
            previewPanel.style.display = '';
        } catch (err) {
            showToast(err.message, 'error');
        }
    });

    document.getElementById('download-btn').addEventListener('click', () => {
        if (!lastReportData) return;
        const format = document.getElementById('report-format').value;
        const ext = format === 'csv' ? 'csv' : 'json';
        const mime = format === 'csv' ? 'text/csv' : 'application/json';
        const blob = new Blob([lastReportData], { type: mime });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `report_${selectedType}_${new Date().toISOString().slice(0, 10)}.${ext}`;
        a.click();
        URL.revokeObjectURL(url);
    });

    document.getElementById('save-report-btn').addEventListener('click', async () => {
        if (!selectedType) return;
        const title = prompt(_('reports.report_name'));
        if (!title) return;
        try {
            const resp = await apiFetch('/api/reports/save', {
                method: 'POST',
                body: JSON.stringify({
                    title,
                    type: selectedType,
                    from: document.getElementById('report-from').value || undefined,
                    to: document.getElementById('report-to').value || undefined,
                })
            });
            await resp.json();
            showToast(_('common.saved'), 'success');
        } catch (err) { showToast(err.message, 'error'); }
    });

    // ==== Saved reports ====

    const savedBody = document.getElementById('saved-body');

    async function loadSaved() {
        savedBody.innerHTML = `<tr class="loading-row"><td colspan="4">${_('common.loading')}</td></tr>`;
        try {
            const resp = await apiFetch('/api/reports/saved');
            const data = await resp.json();
            const reports = Array.isArray(data) ? data : [];
            if (!reports.length) {
                savedBody.innerHTML = `<tr class="empty-row"><td colspan="4">
                    <div class="empty-state"><span class="material-icons">description</span><p>${_('reports.no_saved')}</p></div></td></tr>`;
                return;
            }
            savedBody.innerHTML = reports.map(r => `
                <tr>
                    <td class="report-name">${escapeHtml(r.title || r.name)}</td>
                    <td>${escapeHtml(r.report_type || r.type)}</td>
                    <td class="time-cell">${formatTimeAgo(r.created_at)}</td>
                    <td class="action-btn-group">
                        <button class="btn btn-sm btn-primary run-btn" data-id="${r.id}"><span class="material-icons" style="font-size:16px">play_arrow</span></button>
                        <button class="btn btn-sm btn-danger del-btn" data-id="${r.id}"><span class="material-icons" style="font-size:16px">delete</span></button>
                    </td>
                </tr>
            `).join('');

            savedBody.querySelectorAll('.run-btn').forEach(btn => {
                btn.addEventListener('click', () => runSaved(btn.dataset.id));
            });
            savedBody.querySelectorAll('.del-btn').forEach(btn => {
                btn.addEventListener('click', () => deleteSaved(btn.dataset.id));
            });
        } catch (err) {
            savedBody.innerHTML = `<tr class="empty-row"><td colspan="4">
                <div class="empty-state"><span class="material-icons">error_outline</span><p>${escapeHtml(err.message)}</p></div></td></tr>`;
        }
    }

    async function runSaved(id) {
        try {
            const resp = await apiFetch(`/api/reports/saved/${id}`);
            const saved = await resp.json();
            const payload = typeof saved.payload === 'string'
                ? JSON.parse(saved.payload) : (saved.payload || {});
            lastReportData = JSON.stringify(payload, null, 2);
            renderReport(payload);
            previewPanel.style.display = '';

            // Switch to generate tab
            document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
            document.querySelectorAll('.tab-panel').forEach(p => p.classList.remove('active'));
            document.querySelector('.tab-btn[data-tab="generate"]').classList.add('active');
            document.getElementById('tab-generate').classList.add('active');
        } catch (err) { showToast(err.message, 'error'); }
    }

    async function deleteSaved(id) {
        if (!confirm(_('reports.delete_saved') + '?')) return;
        try {
            await apiFetch(`/api/reports/saved/${id}`, { method: 'DELETE' });
            showToast(_('common.success'), 'success');
            loadSaved();
        } catch (err) { showToast(err.message, 'error'); }
    }

    // ==== Human-readable report renderer ====

    function renderReport(data) {
        if (!data) { previewContent.textContent = ''; return; }
        let html = '';

        // Top-level scalar fields as summary cards
        const scalars = {};
        const sections = {};
        for (const [k, v] of Object.entries(data)) {
            if (v === null || v === undefined) continue;
            if (Array.isArray(v)) { sections[k] = v; }
            else if (typeof v === 'object') { sections[k] = v; }
            else { scalars[k] = v; }
        }

        if (Object.keys(scalars).length > 0) {
            html += '<div class="report-summary">';
            for (const [k, v] of Object.entries(scalars)) {
                html += `<div class="report-stat"><span class="report-stat-label">${escapeHtml(formatKey(k))}</span>`
                    + `<span class="report-stat-value">${escapeHtml(String(v))}</span></div>`;
            }
            html += '</div>';
        }

        // Array sections as tables
        for (const [k, v] of Object.entries(sections)) {
            if (Array.isArray(v) && v.length > 0 && typeof v[0] === 'object') {
                const cols = Object.keys(v[0]);
                html += `<h4 class="report-section-title">${escapeHtml(formatKey(k))}</h4>`;
                html += '<div class="table-responsive"><table class="data-table"><thead><tr>';
                cols.forEach(c => { html += `<th>${escapeHtml(formatKey(c))}</th>`; });
                html += '</tr></thead><tbody>';
                v.forEach(row => {
                    html += '<tr>';
                    cols.forEach(c => { html += `<td>${escapeHtml(String(row[c] ?? ''))}</td>`; });
                    html += '</tr>';
                });
                html += '</tbody></table></div>';
            } else if (Array.isArray(v) && v.length > 0) {
                html += `<h4 class="report-section-title">${escapeHtml(formatKey(k))}</h4>`;
                html += '<ul class="report-list">';
                v.forEach(item => { html += `<li>${escapeHtml(String(item))}</li>`; });
                html += '</ul>';
            } else if (typeof v === 'object' && !Array.isArray(v)) {
                html += `<h4 class="report-section-title">${escapeHtml(formatKey(k))}</h4>`;
                html += '<div class="report-kv">';
                for (const [sk, sv] of Object.entries(v)) {
                    html += `<div class="report-kv-row"><span class="report-kv-key">${escapeHtml(formatKey(sk))}</span>`
                        + `<span class="report-kv-val">${escapeHtml(String(sv ?? ''))}</span></div>`;
                }
                html += '</div>';
            }
        }

        if (!html) { html = `<pre>${escapeHtml(JSON.stringify(data, null, 2))}</pre>`; }

        // Add toggle for raw JSON
        html += `<details class="raw-json-toggle"><summary>${_('reports.raw_json') || 'Raw JSON'}</summary>`
            + `<pre>${escapeHtml(JSON.stringify(data, null, 2))}</pre></details>`;

        previewContent.innerHTML = html;
    }

    function formatKey(key) {
        return key.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
    }

    // ---- Init ----
    // nothing to load on generate tab start
})();

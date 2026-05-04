/**
 * Yomie Console - Tenants Page Script
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

    // ==== Grid ====

    const grid = document.getElementById('tenants-grid');
    const searchInput = document.getElementById('tenant-search');
    const createBtn = document.getElementById('create-tenant-btn');
    const formModal = document.getElementById('form-modal');
    const saveBtn = document.getElementById('tenant-save-btn');
    const cancelBtn = document.getElementById('tenant-cancel-btn');

    let allTenants = [];

    async function loadTenants() {
        grid.innerHTML = `<div class="tenants-empty"><span class="material-icons">hourglass_empty</span><p>${_('common.loading')}</p></div>`;
        try {
            const data = await apiFetch('/api/tenants');
            allTenants = Array.isArray(data) ? data : [];
            filterAndRender();
            updateStats();
        } catch (err) {
            grid.innerHTML = `<div class="tenants-empty"><span class="material-icons">error_outline</span><p>${escapeHtml(err.message)}</p></div>`;
        }
    }

    function updateStats() {
        const total = allTenants.length;
        const active = allTenants.filter(t => t.active).length;
        const devices = allTenants.reduce((sum, t) => sum + (t.device_count || 0), 0);
        const users = allTenants.reduce((sum, t) => sum + (t.user_count || 0), 0);
        document.getElementById('stat-tenants').textContent = total;
        document.getElementById('stat-active').textContent = active;
        document.getElementById('stat-devices').textContent = devices;
        document.getElementById('stat-users').textContent = users;
    }

    function filterAndRender() {
        const q = searchInput.value.toLowerCase();
        const filtered = q ? allTenants.filter(t => (t.name + t.slug + (t.contact_name || '') + (t.contact_email || '')).toLowerCase().includes(q)) : allTenants;
        renderGrid(filtered);
    }

    function renderGrid(tenants) {
        if (!tenants.length) {
            grid.innerHTML = `<div class="tenants-empty"><span class="material-icons">business</span><p>${_('tenants.no_tenants')}</p></div>`;
            return;
        }
        grid.innerHTML = tenants.map(t => {
            const initial = (t.name || '?').charAt(0).toUpperCase();
            return `
            <div class="tenant-card" data-id="${t.id}">
                <div class="tenant-card-header">
                    <h3>
                        <span style="display:inline-flex;align-items:center;justify-content:center;width:32px;height:32px;border-radius:var(--radius-md);background:var(--accent-blue-muted);color:var(--accent-blue);font-weight:600;font-size:14px;flex-shrink:0">${escapeHtml(initial)}</span>
                        ${escapeHtml(t.name)}
                    </h3>
                    <span class="tenant-status ${t.active ? 'active' : 'disabled'}"><span class="dot"></span> ${t.active ? _('tenants.active') : _('tenants.disabled')}</span>
                </div>
                <div class="tenant-card-stats">
                    <div class="tenant-stat">
                        <span class="value">${t.device_count || 0}</span>
                        <span class="label">${_('tenants.devices')}</span>
                    </div>
                    <div class="tenant-stat">
                        <span class="value">${t.user_count || 0}</span>
                        <span class="label">${_('tenants.users')}</span>
                    </div>
                    <div class="tenant-stat">
                        <span class="value">${t.max_devices || '∞'}</span>
                        <span class="label">${_('tenants.max')}</span>
                    </div>
                </div>
                <div class="tenant-card-footer">
                    <span class="tenant-slug">${escapeHtml(t.slug || '')}</span>
                    <div class="tenant-card-actions">
                        <button class="btn btn-sm btn-secondary edit-btn" data-id="${t.id}" title="${_('common.edit')}"><span class="material-icons" style="font-size:16px">edit</span></button>
                        <button class="btn btn-sm btn-danger del-btn" data-id="${t.id}" title="${_('common.delete')}"><span class="material-icons" style="font-size:16px">delete</span></button>
                    </div>
                </div>
            </div>`;
        }).join('');

        // Card click → detail
        grid.querySelectorAll('.tenant-card').forEach(card => {
            card.addEventListener('click', (e) => {
                if (e.target.closest('.edit-btn') || e.target.closest('.del-btn')) return;
                showDetail(card.dataset.id);
            });
        });

        grid.querySelectorAll('.edit-btn').forEach(btn => {
            btn.addEventListener('click', (e) => { e.stopPropagation(); editTenant(btn.dataset.id); });
        });
        grid.querySelectorAll('.del-btn').forEach(btn => {
            btn.addEventListener('click', (e) => { e.stopPropagation(); deleteTenant(btn.dataset.id); });
        });
    }

    // ==== Form ====

    function openForm(t) {
        document.getElementById('form-modal-title').textContent = t ? _('tenants.edit_tenant') : _('tenants.create_tenant');
        document.getElementById('tenant-id').value = t ? t.id : '';
        document.getElementById('tenant-name').value = t ? t.name : '';
        document.getElementById('tenant-slug').value = t ? t.slug || '' : '';
        document.getElementById('tenant-contact-name').value = t ? t.contact_name || '' : '';
        document.getElementById('tenant-contact-email').value = t ? t.contact_email || '' : '';
        document.getElementById('tenant-max-devices').value = t ? t.max_devices || 100 : 100;
        document.getElementById('tenant-active').checked = t ? !!t.active : true;
        formModal.style.display = 'flex';
    }

    async function editTenant(id) {
        try {
            const t = await apiFetch(`/api/tenants/${id}`);
            openForm(t);
        } catch (err) { showToast(err.message, 'error'); }
    }

    async function saveTenant() {
        const id = document.getElementById('tenant-id').value;
        const body = {
            name: document.getElementById('tenant-name').value.trim(),
            slug: document.getElementById('tenant-slug').value.trim(),
            contact_name: document.getElementById('tenant-contact-name').value.trim(),
            contact_email: document.getElementById('tenant-contact-email').value.trim(),
            max_devices: parseInt(document.getElementById('tenant-max-devices').value, 10) || 100,
            active: document.getElementById('tenant-active').checked,
        };
        if (!body.name) { showToast(_('common.name_required'), 'error'); return; }

        try {
            const url = id ? `/api/tenants/${id}` : '/api/tenants';
            const method = id ? 'PATCH' : 'POST';
            await apiFetch(url, { method, body: JSON.stringify(body) });
            formModal.style.display = 'none';
            showToast(_('common.saved'), 'success');
            loadTenants();
        } catch (err) { showToast(err.message, 'error'); }
    }

    async function deleteTenant(id) {
        if (!confirm(_('tenants.delete_tenant') + '?')) return;
        try {
            await apiFetch(`/api/tenants/${id}`, { method: 'DELETE' });
            showToast(_('common.success'), 'success');
            loadTenants();
        } catch (err) { showToast(err.message, 'error'); }
    }

    createBtn.addEventListener('click', () => openForm(null));
    saveBtn.addEventListener('click', saveTenant);
    cancelBtn?.addEventListener('click', () => { formModal.style.display = 'none'; });
    searchInput.addEventListener('input', filterAndRender);

    // ==== Detail ====

    async function showDetail(id) {
        const modal = document.getElementById('detail-modal');
        const body = document.getElementById('detail-body');
        modal.style.display = 'flex';
        body.innerHTML = `<p>${_('common.loading')}</p>`;

        try {
            const [tenant, devices, users, stats] = await Promise.all([
                apiFetch(`/api/tenants/${id}`),
                apiFetch(`/api/tenants/${id}/devices`),
                apiFetch(`/api/tenants/${id}/users`),
                apiFetch(`/api/tenants/${id}/stats`).catch(() => ({}))
            ]);

            document.getElementById('detail-modal-title').textContent = tenant.name;

            const devList = Array.isArray(devices) ? devices : [];
            const usrList = Array.isArray(users) ? users : [];

            body.innerHTML = `
                <div class="tenant-detail-tabs">
                    <button class="tenant-detail-tab active" data-panel="info"><span class="material-icons" style="font-size:16px;vertical-align:-3px">info</span> ${_('tenants.info')}</button>
                    <button class="tenant-detail-tab" data-panel="devices"><span class="material-icons" style="font-size:16px;vertical-align:-3px">devices</span> ${_('tenants.devices')} (${devList.length})</button>
                    <button class="tenant-detail-tab" data-panel="users"><span class="material-icons" style="font-size:16px;vertical-align:-3px">people</span> ${_('tenants.users')} (${usrList.length})</button>
                </div>

                <div class="tenant-detail-panel active" id="dp-info">
                    <div class="detail-grid">
                        <div class="detail-item"><span class="detail-label">${_('tenants.name')}</span><span class="detail-value">${escapeHtml(tenant.name)}</span></div>
                        <div class="detail-item"><span class="detail-label">${_('tenants.slug')}</span><span class="detail-value"><code style="font-size:var(--font-size-sm);padding:2px 6px;background:var(--bg-tertiary);border-radius:var(--radius-sm)">${escapeHtml(tenant.slug || '—')}</code></span></div>
                        <div class="detail-item"><span class="detail-label">${_('tenants.contact_name')}</span><span class="detail-value">${escapeHtml(tenant.contact_name || '—')}</span></div>
                        <div class="detail-item"><span class="detail-label">${_('tenants.contact_email')}</span><span class="detail-value">${tenant.contact_email ? `<a href="mailto:${escapeHtml(tenant.contact_email)}" style="color:var(--accent-blue)">${escapeHtml(tenant.contact_email)}</a>` : '—'}</span></div>
                        <div class="detail-item"><span class="detail-label">${_('tenants.max_devices')}</span><span class="detail-value">${tenant.max_devices || '∞'}</span></div>
                        <div class="detail-item"><span class="detail-label">${_('tenants.status')}</span><span class="detail-value"><span class="tenant-status ${tenant.active ? 'active' : 'disabled'}"><span class="dot"></span> ${tenant.active ? _('tenants.active') : _('tenants.disabled')}</span></span></div>
                    </div>
                </div>

                <div class="tenant-detail-panel" id="dp-devices">
                    ${devList.length
                        ? `<div class="assignment-list">${devList.map(d => `
                            <div class="assignment-item">
                                <span class="name"><span class="material-icons" style="font-size:16px;vertical-align:-3px;margin-right:4px;color:var(--text-muted)">computer</span>${escapeHtml(d.id || d.device_id)}</span>
                                <span style="color:var(--text-muted);font-size:var(--font-size-sm)">${escapeHtml(d.hostname || '')}</span>
                            </div>`).join('')}</div>`
                        : `<p class="text-muted" style="text-align:center;padding:var(--space-lg)">${_('tenants.no_devices')}</p>`
                    }
                </div>

                <div class="tenant-detail-panel" id="dp-users">
                    ${usrList.length
                        ? `<div class="assignment-list">${usrList.map(u => `
                            <div class="assignment-item">
                                <span class="name"><span class="material-icons" style="font-size:16px;vertical-align:-3px;margin-right:4px;color:var(--text-muted)">person</span>${escapeHtml(u.username || u.name)}</span>
                                <span style="color:var(--text-muted);font-size:var(--font-size-sm)">${escapeHtml(u.role || '')}</span>
                            </div>`).join('')}</div>`
                        : `<p class="text-muted" style="text-align:center;padding:var(--space-lg)">${_('tenants.no_users')}</p>`
                    }
                </div>
            `;

            // Detail tab switching
            body.querySelectorAll('.tenant-detail-tab').forEach(tab => {
                tab.addEventListener('click', () => {
                    body.querySelectorAll('.tenant-detail-tab').forEach(t => t.classList.remove('active'));
                    body.querySelectorAll('.tenant-detail-panel').forEach(p => p.classList.remove('active'));
                    tab.classList.add('active');
                    const panel = document.getElementById('dp-' + tab.dataset.panel);
                    if (panel) panel.classList.add('active');
                });
            });
        } catch (err) {
            body.innerHTML = `<p class="text-error">${escapeHtml(err.message)}</p>`;
        }
    }

    // ---- Init ----
    loadTenants();
})();

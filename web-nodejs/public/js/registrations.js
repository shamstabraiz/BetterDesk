/**
 * Yomie Console - Registrations Page Script
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

    // State
    let currentStatus = '';
    let currentSearch = '';
    let rejectTargetId = null;

    // ---- DOM refs ----
    const tbody = document.getElementById('registrations-body');
    const searchInput = document.getElementById('search-input');
    const pendingCountBadge = document.getElementById('pending-count');
    const filterButtons = document.querySelectorAll('.filter-btn');
    const rejectModal = document.getElementById('reject-modal');
    const rejectReasonInput = document.getElementById('reject-reason');
    const confirmRejectBtn = document.getElementById('confirm-reject-btn');

    // ---- API helpers ----

    async function apiFetch(url, options = {}) {
        const headers = { 'Content-Type': 'application/json' };
        if (csrfToken) headers['x-csrf-token'] = csrfToken;
        const resp = await fetch(url, { ...options, headers: { ...headers, ...options.headers } });
        return resp.json();
    }

    // ---- Load registrations ----

    async function loadRegistrations() {
        const params = new URLSearchParams();
        if (currentStatus) params.set('status', currentStatus);
        if (currentSearch) params.set('search', currentSearch);

        tbody.innerHTML = `
            <tr class="loading-row">
                <td colspan="8">${_('common.loading')}</td>
            </tr>
        `;

        try {
            const result = await apiFetch(`/api/registrations?${params}`);
            if (!result.success) throw new Error(result.error);

            renderTable(result.data || []);
        } catch (err) {
            tbody.innerHTML = `
                <tr class="empty-row">
                    <td colspan="8">
                        <div class="empty-state">
                            <span class="material-icons">error_outline</span>
                            <p>${err.message || _('errors.server_error')}</p>
                        </div>
                    </td>
                </tr>
            `;
        }
    }

    async function loadPendingCount() {
        try {
            const result = await apiFetch('/api/registrations/count');
            const count = result.count || 0;
            pendingCountBadge.textContent = count;
            pendingCountBadge.style.display = count > 0 ? '' : 'none';

            // Update sidebar badge too
            const sidebarBadge = document.getElementById('reg-sidebar-badge');
            if (sidebarBadge) {
                sidebarBadge.textContent = count;
                sidebarBadge.style.display = count > 0 ? '' : 'none';
            }
        } catch (_) { /* silent */ }
    }

    // ---- Render ----

    function renderTable(registrations) {
        if (!registrations.length) {
            tbody.innerHTML = `
                <tr class="empty-row">
                    <td colspan="8">
                        <div class="empty-state">
                            <span class="material-icons">how_to_reg</span>
                            <p>${_('registrations.no_registrations')}</p>
                        </div>
                    </td>
                </tr>
            `;
            return;
        }

        tbody.innerHTML = registrations.map(reg => {
            const statusClass = reg.status;
            const statusLabel = _(`registrations.status_${reg.status}`) || reg.status;
            const platformIcon = getPlatformIcon(reg.platform);
            const timeAgo = formatTimeAgo(reg.created_at);

            let actions = '';
            if (reg.status === 'pending') {
                actions = `
                    <button class="action-btn approve" data-reg-action="approve" data-id="${reg.id}" title="${_('registrations.approve_btn')}">
                        <span class="material-icons">check</span>
                        ${_('registrations.approve_btn')}
                    </button>
                    <button class="action-btn reject" data-reg-action="reject" data-id="${reg.id}" title="${_('registrations.reject_btn')}">
                        <span class="material-icons">close</span>
                        ${_('registrations.reject_btn')}
                    </button>
                `;
            } else {
                actions = `
                    <button class="action-btn delete" data-reg-action="remove" data-id="${reg.id}" title="${_('common.delete')}">
                        <span class="material-icons">delete</span>
                    </button>
                `;
            }

            return `
                <tr data-id="${reg.id}">
                    <td class="device-id-cell">${escapeHtml(reg.device_id)}</td>
                    <td>${escapeHtml(reg.hostname || '—')}</td>
                    <td class="platform-cell">
                        <span class="material-icons">${platformIcon}</span>
                        ${escapeHtml(reg.platform || '—')}
                    </td>
                    <td>${escapeHtml(reg.ip_address || '—')}</td>
                    <td>${escapeHtml(reg.version || '—')}</td>
                    <td><span class="status-badge ${statusClass}">${statusLabel}</span></td>
                    <td class="time-cell" title="${escapeHtml(reg.created_at || '')}">${timeAgo}</td>
                    <td class="action-btn-group">${actions}</td>
                </tr>
            `;
        }).join('');
    }

    // ---- Actions ----

    async function approveRegistration(id) {
        if (!confirm(_('registrations.approve_confirm'))) return;

        try {
            const result = await apiFetch(`/api/registrations/${id}/approve`, { method: 'PUT' });
            if (!result.success) throw new Error(result.error);

            showToast(_('registrations.approved_success'), 'success');
            loadRegistrations();
            loadPendingCount();
        } catch (err) {
            showToast(err.message || _('errors.server_error'), 'error');
        }
    }

    function openRejectModal(id) {
        rejectTargetId = id;
        rejectReasonInput.value = '';
        rejectModal.style.display = 'flex';
    }

    async function confirmReject() {
        if (!rejectTargetId) return;

        try {
            const result = await apiFetch(`/api/registrations/${rejectTargetId}/reject`, {
                method: 'PUT',
                body: JSON.stringify({ reason: rejectReasonInput.value }),
            });
            if (!result.success) throw new Error(result.error);

            rejectModal.style.display = 'none';
            rejectTargetId = null;
            showToast(_('registrations.rejected_success'), 'success');
            loadRegistrations();
            loadPendingCount();
        } catch (err) {
            showToast(err.message || _('errors.server_error'), 'error');
        }
    }

    async function deleteRegistration(id) {
        if (!confirm(_('registrations.delete_confirm'))) return;

        try {
            const result = await apiFetch(`/api/registrations/${id}`, { method: 'DELETE' });
            if (!result.success) throw new Error(result.error);

            showToast(_('registrations.deleted_success'), 'success');
            loadRegistrations();
            loadPendingCount();
        } catch (err) {
            showToast(err.message || _('errors.server_error'), 'error');
        }
    }

    // ---- Helpers ----

    function escapeHtml(str) {
        const div = document.createElement('div');
        div.textContent = str;
        return div.innerHTML;
    }

    function getPlatformIcon(platform) {
        if (!platform) return 'devices';
        const p = platform.toLowerCase();
        if (p.includes('windows')) return 'laptop_windows';
        if (p.includes('linux')) return 'computer';
        if (p.includes('mac') || p.includes('darwin')) return 'laptop_mac';
        if (p.includes('android')) return 'phone_android';
        if (p.includes('ios')) return 'phone_iphone';
        return 'devices';
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
        // Use Yomie notification system if available
        if (window.Yomie?.notify) {
            window.Yomie.notify(message, type);
            return;
        }
        // Fallback: use toast container
        const container = document.getElementById('toast-container');
        if (!container) return;
        const toast = document.createElement('div');
        toast.className = `toast toast-${type || 'info'}`;
        toast.textContent = message;
        container.appendChild(toast);
        setTimeout(() => toast.remove(), 4000);
    }

    // ---- Event listeners ----

    // Filter buttons
    filterButtons.forEach(btn => {
        btn.addEventListener('click', () => {
            filterButtons.forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            currentStatus = btn.dataset.status;
            loadRegistrations();
        });
    });

    // Search
    let searchTimeout;
    searchInput.addEventListener('input', () => {
        clearTimeout(searchTimeout);
        searchTimeout = setTimeout(() => {
            currentSearch = searchInput.value.trim();
            loadRegistrations();
        }, 300);
    });

    // Reject modal
    confirmRejectBtn.addEventListener('click', confirmReject);

    tbody.addEventListener('click', (e) => {
        const actionBtn = e.target.closest('[data-reg-action]');
        if (!actionBtn) return;
        const id = actionBtn.dataset.id;
        switch (actionBtn.dataset.regAction) {
            case 'approve':
                approveRegistration(id);
                break;
            case 'reject':
                openRejectModal(id);
                break;
            case 'remove':
                deleteRegistration(id);
                break;
        }
    });

    // Close modal buttons
    document.querySelectorAll('.modal-close').forEach(btn => {
        btn.addEventListener('click', () => {
            const modalId = btn.dataset.modal;
            if (modalId) document.getElementById(modalId).style.display = 'none';
        });
    });

    // ---- Init ----

    loadRegistrations();
    loadPendingCount();

    // Refresh pending count every 15 seconds
    setInterval(loadPendingCount, 15000);

})();

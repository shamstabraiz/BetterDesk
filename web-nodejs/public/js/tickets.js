/**
 * Yomie Console - Tickets Page Script
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
    const user = window.Yomie?.user || {};

    // State
    let currentStatus = '';
    let currentSearch = '';

    // DOM refs
    const tbody = document.getElementById('tickets-body');
    const searchInput = document.getElementById('search-input');
    const filterButtons = document.querySelectorAll('.filter-btn');
    const createBtn = document.getElementById('create-ticket-btn');
    const ticketModal = document.getElementById('ticket-modal');
    const ticketModalTitle = document.getElementById('ticket-modal-title');
    const ticketForm = document.getElementById('ticket-form');
    const ticketSaveBtn = document.getElementById('ticket-save-btn');
    const detailModal = document.getElementById('detail-modal');
    const detailBody = document.getElementById('detail-body');
    const statTotal = document.getElementById('stat-total');
    const statOpen = document.getElementById('stat-open');
    const statProgress = document.getElementById('stat-progress');
    const statResolved = document.getElementById('stat-resolved');

    // ---- API ----

    async function apiFetch(url, options = {}) {
        const headers = { 'Content-Type': 'application/json' };
        if (csrfToken) headers['x-csrf-token'] = csrfToken;
        const resp = await fetch(url, { ...options, headers: { ...headers, ...options.headers } });
        return resp.json();
    }

    // ---- Load ----

    async function loadTickets() {
        tbody.innerHTML = `<tr class="loading-row"><td colspan="9">${_('common.loading')}</td></tr>`;

        const params = new URLSearchParams();
        if (currentStatus) params.set('status', currentStatus);
        if (currentSearch) params.set('search', currentSearch);

        try {
            const result = await apiFetch(`/api/tickets?${params}`);
            const tickets = result.tickets || [];
            renderTable(tickets);
        } catch (err) {
            tbody.innerHTML = `<tr class="empty-row"><td colspan="9">
                <div class="empty-state"><span class="material-icons">error_outline</span>
                <p>${escapeHtml(err.message)}</p></div></td></tr>`;
        }
    }

    async function loadStats() {
        try {
            const stats = await apiFetch('/api/tickets/stats');
            statTotal.textContent = stats.total || 0;
            statOpen.textContent = stats.open || 0;
            statProgress.textContent = stats.in_progress || 0;
            statResolved.textContent = stats.resolved || 0;
        } catch (_e) { /* silent */ }
    }

    // ---- Render ----

    function renderTable(tickets) {
        if (!tickets.length) {
            tbody.innerHTML = `<tr class="empty-row"><td colspan="9">
                <div class="empty-state"><span class="material-icons">confirmation_number</span>
                <p>${_('tickets.no_tickets')}</p></div></td></tr>`;
            return;
        }

        tbody.innerHTML = tickets.map(t => {
            const statusLabel = _(`tickets.status_${t.status}`) || t.status;
            const priorityLabel = _(`tickets.priority_${t.priority}`) || t.priority;
            const catLabel = _(`tickets.category_${t.category}`) || t.category || '—';
            const slaOverdue = t.sla_due_at && new Date(t.sla_due_at) < new Date() && t.status !== 'closed' && t.status !== 'resolved';

            return `<tr data-id="${t.id}">
                <td>#${t.id}</td>
                <td>
                    <a href="javascript:void(0)" class="ticket-link" data-id="${t.id}">${escapeHtml(t.title)}</a>
                    ${slaOverdue ? `<span class="sla-overdue">${_('tickets.sla_overdue')}</span>` : ''}
                </td>
                <td><span class="status-badge ${t.status}">${statusLabel}</span></td>
                <td><span class="priority-badge ${t.priority}">${priorityLabel}</span></td>
                <td>${escapeHtml(catLabel)}</td>
                <td>${escapeHtml(t.assigned_to || '—')}</td>
                <td>${escapeHtml(t.device_id || '—')}</td>
                <td class="time-cell">${formatTimeAgo(t.created_at)}</td>
                <td class="action-btn-group">
                    <button class="btn btn-sm btn-secondary view-btn" data-id="${t.id}" title="${_('tickets.detail')}">
                        <span class="material-icons" style="font-size:16px">visibility</span>
                    </button>
                    <button class="btn btn-sm btn-secondary edit-btn" data-id="${t.id}" title="${_('tickets.edit')}">
                        <span class="material-icons" style="font-size:16px">edit</span>
                    </button>
                    <button class="btn btn-sm btn-danger delete-btn" data-id="${t.id}" title="${_('tickets.delete')}">
                        <span class="material-icons" style="font-size:16px">delete</span>
                    </button>
                </td>
            </tr>`;
        }).join('');

        attachRowListeners();
    }

    function attachRowListeners() {
        tbody.querySelectorAll('.ticket-link, .view-btn').forEach(el => {
            el.addEventListener('click', () => openDetail(el.dataset.id));
        });
        tbody.querySelectorAll('.edit-btn').forEach(el => {
            el.addEventListener('click', () => openEdit(el.dataset.id));
        });
        tbody.querySelectorAll('.delete-btn').forEach(el => {
            el.addEventListener('click', () => deleteTicket(el.dataset.id));
        });
    }

    // ---- Detail ----

    async function openDetail(id) {
        detailModal.style.display = 'flex';
        detailBody.innerHTML = `<p>${_('common.loading')}</p>`;

        try {
            const t = await apiFetch(`/api/tickets/${encodeURIComponent(id)}`);
            renderDetail(t);
        } catch (err) {
            detailBody.innerHTML = `<p class="text-error">${escapeHtml(err.message)}</p>`;
        }
    }

    function renderDetail(t) {
        const statusLabel = _(`tickets.status_${t.status}`) || t.status;
        const priorityLabel = _(`tickets.priority_${t.priority}`) || t.priority;
        const catLabel = _(`tickets.category_${t.category}`) || t.category || '—';

        let html = `
            <div class="detail-section">
                <div class="detail-grid">
                    <div class="detail-item"><span class="detail-label">${_('tickets.status')}</span><span class="status-badge ${t.status}">${statusLabel}</span></div>
                    <div class="detail-item"><span class="detail-label">${_('tickets.priority')}</span><span class="priority-badge ${t.priority}">${priorityLabel}</span></div>
                    <div class="detail-item"><span class="detail-label">${_('tickets.category')}</span><span class="detail-value">${escapeHtml(catLabel)}</span></div>
                    <div class="detail-item"><span class="detail-label">${_('tickets.assigned_to')}</span><span class="detail-value">${escapeHtml(t.assigned_to || '—')}</span></div>
                    <div class="detail-item"><span class="detail-label">${_('tickets.device')}</span><span class="detail-value">${escapeHtml(t.device_id || '—')}</span></div>
                    <div class="detail-item"><span class="detail-label">${_('tickets.created_by')}</span><span class="detail-value">${escapeHtml(t.created_by || '—')}</span></div>
                    <div class="detail-item"><span class="detail-label">${_('tickets.created_at')}</span><span class="detail-value">${formatDate(t.created_at)}</span></div>
                    <div class="detail-item"><span class="detail-label">${_('tickets.sla_due')}</span><span class="detail-value">${formatDate(t.sla_due_at)}</span></div>
                </div>
            </div>`;

        if (t.description) {
            html += `<div class="detail-section"><h4>${_('tickets.description')}</h4><p style="white-space:pre-wrap;font-size:0.875rem">${escapeHtml(t.description)}</p></div>`;
        }

        // Status change buttons
        html += `<div class="detail-section" style="display:flex;gap:8px;flex-wrap:wrap">`;
        if (t.status === 'open') {
            html += `<button class="btn btn-sm btn-secondary" data-ticket-detail-action="change-status" data-id="${t.id}" data-status="in_progress">${_('tickets.status_in_progress')}</button>`;
        }
        if (t.status === 'in_progress') {
            html += `<button class="btn btn-sm btn-primary" data-ticket-detail-action="change-status" data-id="${t.id}" data-status="resolved">${_('tickets.status_resolved')}</button>`;
        }
        if (t.status !== 'closed') {
            html += `<button class="btn btn-sm btn-secondary" data-ticket-detail-action="change-status" data-id="${t.id}" data-status="closed">${_('tickets.status_closed')}</button>`;
        }
        if (!t.assigned_to && user.username) {
            html += `<button class="btn btn-sm btn-secondary" data-ticket-detail-action="assign" data-id="${t.id}">${_('tickets.assign_to_me')}</button>`;
        }
        html += `</div>`;

        // Comments
        html += `<div class="comments-section"><h4>${_('tickets.comments')}</h4>`;
        if (t.comments && t.comments.length) {
            t.comments.forEach(c => {
                html += `<div class="comment-item ${c.is_internal ? 'comment-internal' : ''}">
                    <div class="comment-header">
                        <span class="comment-author">${escapeHtml(c.author || '—')}</span>
                        <span class="comment-time">${formatTimeAgo(c.created_at)}</span>
                    </div>
                    <div class="comment-body">${escapeHtml(c.body)}</div>
                </div>`;
            });
        } else {
            html += `<p style="color:var(--text-secondary);font-size:0.875rem">${_('tickets.no_comments')}</p>`;
        }
        html += `<div class="add-comment-form">
            <textarea class="form-input" id="new-comment" placeholder="${_('tickets.comment_placeholder')}" maxlength="2000"></textarea>
            <button class="btn btn-primary" data-ticket-detail-action="add-comment" data-id="${t.id}">${_('tickets.add_comment')}</button>
        </div></div>`;

        detailBody.innerHTML = html;
    }

    // ---- Create / Edit ----

    function openCreateModal() {
        ticketModalTitle.textContent = _('tickets.create');
        ticketForm.reset();
        document.getElementById('ticket-id').value = '';
        ticketModal.style.display = 'flex';
    }

    async function openEdit(id) {
        ticketModalTitle.textContent = _('tickets.edit');
        ticketModal.style.display = 'flex';

        try {
            const t = await apiFetch(`/api/tickets/${encodeURIComponent(id)}`);
            document.getElementById('ticket-id').value = t.id;
            document.getElementById('ticket-title').value = t.title || '';
            document.getElementById('ticket-desc').value = t.description || '';
            document.getElementById('ticket-priority').value = t.priority || 'medium';
            document.getElementById('ticket-category').value = t.category || 'general';
            document.getElementById('ticket-device').value = t.device_id || '';
        } catch (err) {
            showToast(err.message, 'error');
            ticketModal.style.display = 'none';
        }
    }

    async function saveTicket() {
        const id = document.getElementById('ticket-id').value;
        const body = {
            title: document.getElementById('ticket-title').value.trim(),
            description: document.getElementById('ticket-desc').value.trim(),
            priority: document.getElementById('ticket-priority').value,
            category: document.getElementById('ticket-category').value,
            device_id: document.getElementById('ticket-device').value.trim() || undefined,
        };

        if (!body.title) {
            showToast(_('tickets.title_required'), 'error');
            return;
        }

        try {
            const url = id ? `/api/tickets/${id}` : '/api/tickets';
            const method = id ? 'PATCH' : 'POST';
            const result = await apiFetch(url, { method, body: JSON.stringify(body) });
            if (result.error) throw new Error(result.error);

            ticketModal.style.display = 'none';
            showToast(id ? _('tickets.updated_success') : _('tickets.created_success'), 'success');
            loadTickets();
            loadStats();
        } catch (err) {
            showToast(err.message, 'error');
        }
    }

    // ---- Actions ----

    async function deleteTicket(id) {
        const msg = (_('tickets.delete_confirm') || '').replace('{id}', id);
        if (!confirm(msg)) return;

        try {
            const result = await apiFetch(`/api/tickets/${id}`, { method: 'DELETE' });
            if (result.error) throw new Error(result.error);
            showToast(_('tickets.delete_success'), 'success');
            loadTickets();
            loadStats();
        } catch (err) {
            showToast(err.message, 'error');
        }
    }

    async function changeStatus(id, status) {
        try {
            const result = await apiFetch(`/api/tickets/${id}`, {
                method: 'PATCH',
                body: JSON.stringify({ status }),
            });
            if (result.error) throw new Error(result.error);
            showToast(_('tickets.updated_success'), 'success');
            openDetail(id);
            loadTickets();
            loadStats();
        } catch (err) {
            showToast(err.message, 'error');
        }
    }

    async function assignToMe(id) {
        try {
            const result = await apiFetch(`/api/tickets/${id}`, {
                method: 'PATCH',
                body: JSON.stringify({ assigned_to: user.username }),
            });
            if (result.error) throw new Error(result.error);
            showToast(_('tickets.updated_success'), 'success');
            openDetail(id);
            loadTickets();
        } catch (err) {
            showToast(err.message, 'error');
        }
    }

    async function addComment(ticketId) {
        const textarea = document.getElementById('new-comment');
        const body = textarea.value.trim();
        if (!body) return;

        try {
            const result = await apiFetch(`/api/tickets/${ticketId}/comments`, {
                method: 'POST',
                body: JSON.stringify({ body }),
            });
            if (result.error) throw new Error(result.error);
            textarea.value = '';
            openDetail(ticketId);
        } catch (err) {
            showToast(err.message, 'error');
        }
    }

    // ---- Helpers ----

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

    function formatDate(dateStr) {
        if (!dateStr) return '—';
        return new Date(dateStr).toLocaleString();
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

    // ---- Events ----

    filterButtons.forEach(btn => {
        btn.addEventListener('click', () => {
            filterButtons.forEach(b => b.classList.remove('active'));
            btn.classList.add('active');
            currentStatus = btn.dataset.status;
            loadTickets();
        });
    });

    let searchTimeout;
    searchInput.addEventListener('input', () => {
        clearTimeout(searchTimeout);
        searchTimeout = setTimeout(() => {
            currentSearch = searchInput.value.trim();
            loadTickets();
        }, 300);
    });

    createBtn.addEventListener('click', openCreateModal);
    ticketSaveBtn.addEventListener('click', saveTicket);

    detailBody.addEventListener('click', (e) => {
        const actionBtn = e.target.closest('[data-ticket-detail-action]');
        if (!actionBtn) return;
        const id = actionBtn.dataset.id;
        switch (actionBtn.dataset.ticketDetailAction) {
            case 'change-status':
                changeStatus(id, actionBtn.dataset.status);
                break;
            case 'assign':
                assignToMe(id);
                break;
            case 'add-comment':
                addComment(id);
                break;
        }
    });

    document.querySelectorAll('.modal-close').forEach(btn => {
        btn.addEventListener('click', () => {
            const modalId = btn.dataset.modal;
            if (modalId) document.getElementById(modalId).style.display = 'none';
        });
    });

    // ---- Init ----
    loadTickets();
    loadStats();
})();

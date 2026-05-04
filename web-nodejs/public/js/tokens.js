/**
 * Yomie Console - Device Tokens Management
 * Client-side logic for device enrollment token management.
 */
(function () {
    'use strict';

    const _ = window.Yomie?.translations
        ? (key) => key.split('.').reduce((o, k) => (o && o[k]) || key, window.Yomie.translations)
        : (key) => key;

    let allTokens = [];
    let currentFilter = 'all';
    const csrfToken = window.Yomie?.csrfToken || '';

    // ── Helpers ──────────────────────────────────────────────────────────

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
        setTimeout(() => toast.remove(), 3000);
    }

    async function apiFetch(url, opts = {}) {
        const headers = { 'Content-Type': 'application/json', ...(opts.headers || {}) };
        if (csrfToken) headers['x-csrf-token'] = csrfToken;
        const res = await fetch(url, { ...opts, headers, credentials: 'same-origin' });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
    }

    function escapeHtml(str) {
        if (!str) return '';
        const div = document.createElement('div');
        div.textContent = str;
        return div.innerHTML;
    }

    // ── Data Fetching ────────────────────────────────────────────────────

    async function loadTokens() {
        try {
            const includeRevoked = currentFilter === 'revoked' || currentFilter === 'all';
            const res = await apiFetch(`/api/panel/tokens?include_revoked=${includeRevoked}`);
            if (res.success && res.data) {
                allTokens = res.data.tokens || [];
            } else {
                allTokens = [];
            }
            updateStats();
            renderTokens();
        } catch (err) {
            showToast(_('common.error') + ': ' + err.message, 'error');
        }
    }

    async function loadEnrollmentMode() {
        try {
            const res = await apiFetch('/api/panel/enrollment/mode');
            if (res.success && res.data) {
                const mode = res.data.mode || 'open';
                const radio = document.querySelector(`input[name="enrollment_mode"][value="${mode}"]`);
                if (radio) radio.checked = true;
            }
        } catch (err) {
            // Silently fail — mode card will show default
        }
    }

    // ── Rendering ────────────────────────────────────────────────────────

    function updateStats() {
        const total = allTokens.length;
        const active = allTokens.filter(t => t.status === 'pending').length;
        const used = allTokens.filter(t => t.status === 'used').length;
        const revoked = allTokens.filter(t => t.status === 'revoked').length;
        document.getElementById('stat-total').textContent = total;
        document.getElementById('stat-active').textContent = active;
        document.getElementById('stat-used').textContent = used;
        document.getElementById('stat-revoked').textContent = revoked;
    }

    function renderTokens() {
        const tbody = document.getElementById('tokens-body');
        const search = (document.getElementById('token-search')?.value || '').toLowerCase();

        let filtered = allTokens;
        if (currentFilter !== 'all') {
            filtered = filtered.filter(t => t.status === currentFilter);
        }
        if (search) {
            filtered = filtered.filter(t =>
                (t.name || '').toLowerCase().includes(search) ||
                (t.token_prefix || '').toLowerCase().includes(search) ||
                (t.peer_id || '').toLowerCase().includes(search) ||
                (t.note || '').toLowerCase().includes(search)
            );
        }

        if (filtered.length === 0) {
            tbody.innerHTML = `<tr><td colspan="8" class="empty-row">${escapeHtml(_('tokens.no_tokens'))}</td></tr>`;
            return;
        }

        tbody.innerHTML = filtered.map(t => {
            const statusClass = t.status === 'pending' ? 'status-active'
                : t.status === 'used' ? 'status-used'
                : 'status-revoked';
            const statusLabel = _('tokens.status_' + t.status);
            const expiresStr = t.expires_at ? formatTimeAgo(t.expires_at) : '—';
            const isExpired = t.expires_at && new Date(t.expires_at) < new Date();
            const usesStr = t.max_uses === 0 ? `${t.use_count} / ∞` : `${t.use_count} / ${t.max_uses}`;

            return `<tr data-id="${t.id}">
                <td><code class="token-prefix">${escapeHtml(t.token_prefix)}</code></td>
                <td>${escapeHtml(t.name)}</td>
                <td><span class="status-badge ${statusClass}">${escapeHtml(statusLabel)}</span>
                    ${isExpired ? '<span class="status-badge status-expired">' + escapeHtml(_('tokens.expired')) + '</span>' : ''}</td>
                <td>${usesStr}</td>
                <td>${t.peer_id ? '<code>' + escapeHtml(t.peer_id) + '</code>' : '—'}</td>
                <td>${expiresStr}</td>
                <td>${formatTimeAgo(t.created_at)}</td>
                <td class="actions-cell">
                    ${t.status !== 'revoked' ? `
                        <button class="btn-icon" data-token-action="edit" data-id="${t.id}" title="${escapeHtml(_('common.save'))}">
                            <span class="material-icons">edit</span>
                        </button>
                        <button class="btn-icon btn-icon-danger" data-token-action="revoke" data-id="${t.id}" title="${escapeHtml(_('tokens.revoke'))}">
                            <span class="material-icons">block</span>
                        </button>
                    ` : ''}
                </td>
            </tr>`;
        }).join('');
    }

    // ── Modal Handlers ───────────────────────────────────────────────────

    function showCreateModal() {
        document.getElementById('token-modal-title').textContent = _('tokens.create');
        document.getElementById('token-id').value = '';
        document.getElementById('token-name').value = '';
        document.getElementById('token-max-uses').value = '1';
        document.getElementById('token-expires').value = '0';
        document.getElementById('token-note').value = '';
        document.getElementById('token-modal').style.display = 'flex';
    }

    function editToken(id) {
        const token = allTokens.find(t => t.id === id);
        if (!token) return;
        document.getElementById('token-modal-title').textContent = _('tokens.edit');
        document.getElementById('token-id').value = token.id;
        document.getElementById('token-name').value = token.name || '';
        document.getElementById('token-max-uses').value = token.max_uses || 1;
        document.getElementById('token-expires').value = '0';
        document.getElementById('token-note').value = token.note || '';
        document.getElementById('token-modal').style.display = 'flex';
    }

    function closeModal() {
        document.getElementById('token-modal').style.display = 'none';
    }

    function showBulkModal() {
        document.getElementById('bulk-count').value = '5';
        document.getElementById('bulk-prefix').value = 'Device';
        document.getElementById('bulk-max-uses').value = '1';
        document.getElementById('bulk-expires').value = '604800';
        document.getElementById('bulk-modal').style.display = 'flex';
    }

    function closeBulkModal() {
        document.getElementById('bulk-modal').style.display = 'none';
    }

    function closeCreatedModal() {
        document.getElementById('token-created-modal').style.display = 'none';
    }

    function showCreatedTokens(tokens) {
        const list = document.getElementById('created-tokens-list');
        list.innerHTML = (Array.isArray(tokens) ? tokens : [tokens]).map(t => `
            <div class="token-created-item">
                <div class="token-created-name">${escapeHtml(t.name)}</div>
                <div class="token-created-value">
                    <code>${escapeHtml(t.token)}</code>
                    <button class="btn-icon" data-token-action="copy" data-token="${encodeURIComponent(t.token)}">
                        <span class="material-icons">content_copy</span>
                    </button>
                </div>
            </div>
        `).join('');
        document.getElementById('token-created-modal').style.display = 'flex';
    }

    // ── API Actions ──────────────────────────────────────────────────────

    async function saveToken() {
        const id = document.getElementById('token-id').value;
        const body = {
            name: document.getElementById('token-name').value.trim(),
            max_uses: parseInt(document.getElementById('token-max-uses').value, 10) || 1,
            expires_in: parseInt(document.getElementById('token-expires').value, 10) || 0,
            note: document.getElementById('token-note').value.trim(),
        };

        if (!body.name) {
            showToast(_('common.name_required'), 'error');
            return;
        }

        try {
            if (id) {
                await apiFetch(`/api/panel/tokens/${id}`, { method: 'PUT', body: JSON.stringify(body) });
                showToast(_('common.saved'), 'success');
            } else {
                const res = await apiFetch('/api/panel/tokens', { method: 'POST', body: JSON.stringify(body) });
                if (res.success && res.data) {
                    showCreatedTokens(res.data);
                }
                showToast(_('common.saved'), 'success');
            }
            closeModal();
            loadTokens();
        } catch (err) {
            showToast(_('common.error') + ': ' + err.message, 'error');
        }
    }

    async function revokeToken(id) {
        if (!confirm(_('tokens.revoke_confirm'))) return;
        try {
            await apiFetch(`/api/panel/tokens/${id}`, { method: 'DELETE' });
            showToast(_('tokens.revoked_success'), 'success');
            loadTokens();
        } catch (err) {
            showToast(_('common.error') + ': ' + err.message, 'error');
        }
    }

    async function generateBulk() {
        const body = {
            count: parseInt(document.getElementById('bulk-count').value, 10) || 5,
            name_prefix: document.getElementById('bulk-prefix').value.trim() || 'Device',
            max_uses: parseInt(document.getElementById('bulk-max-uses').value, 10) || 1,
            expires_in: parseInt(document.getElementById('bulk-expires').value, 10) || 0,
        };

        if (body.count < 1 || body.count > 100) {
            showToast(_('tokens.count_range'), 'error');
            return;
        }

        try {
            const res = await apiFetch('/api/panel/tokens/generate-bulk', {
                method: 'POST',
                body: JSON.stringify(body),
            });
            if (res.success && res.data && res.data.tokens) {
                showCreatedTokens(res.data.tokens);
            }
            closeBulkModal();
            loadTokens();
        } catch (err) {
            showToast(_('common.error') + ': ' + err.message, 'error');
        }
    }

    async function setEnrollmentMode(mode) {
        try {
            await apiFetch('/api/panel/enrollment/mode', {
                method: 'PUT',
                body: JSON.stringify({ mode }),
            });
            showToast(_('common.saved'), 'success');
        } catch (err) {
            showToast(_('common.error') + ': ' + err.message, 'error');
        }
    }

    function copyToken(token) {
        navigator.clipboard.writeText(token).then(() => {
            showToast(_('common.copied'), 'success');
        }).catch(() => {
            // Fallback
            const ta = document.createElement('textarea');
            ta.value = token;
            document.body.appendChild(ta);
            ta.select();
            document.execCommand('copy');
            document.body.removeChild(ta);
            showToast(_('common.copied'), 'success');
        });
    }

    // ── Init ─────────────────────────────────────────────────────────────

    function init() {
        document.addEventListener('click', (event) => {
            const button = event.target.closest('[data-token-action]');
            if (!button) return;

            switch (button.dataset.tokenAction) {
                case 'show-bulk-modal':
                    showBulkModal();
                    break;
                case 'show-create-modal':
                    showCreateModal();
                    break;
                case 'close-modal':
                    closeModal();
                    break;
                case 'save-token':
                    saveToken();
                    break;
                case 'close-bulk-modal':
                    closeBulkModal();
                    break;
                case 'generate-bulk':
                    generateBulk();
                    break;
                case 'close-created-modal':
                    closeCreatedModal();
                    break;
                case 'edit':
                    editToken(Number(button.dataset.id));
                    break;
                case 'revoke':
                    revokeToken(Number(button.dataset.id));
                    break;
                case 'copy':
                    copyToken(decodeURIComponent(button.dataset.token || ''));
                    break;
            }
        });

        // Filter pills
        document.querySelectorAll('.filter-pill').forEach(btn => {
            btn.addEventListener('click', () => {
                document.querySelectorAll('.filter-pill').forEach(b => b.classList.remove('active'));
                btn.classList.add('active');
                currentFilter = btn.dataset.filter;
                renderTokens();
            });
        });

        // Search
        const searchInput = document.getElementById('token-search');
        if (searchInput) {
            searchInput.addEventListener('input', () => renderTokens());
        }

        // Enrollment mode radios
        document.querySelectorAll('input[name="enrollment_mode"]').forEach(radio => {
            radio.addEventListener('change', (e) => {
                setEnrollmentMode(e.target.value);
            });
        });

        // Close modals on overlay click
        document.querySelectorAll('.modal-overlay').forEach(overlay => {
            overlay.addEventListener('click', (e) => {
                if (e.target === overlay) overlay.style.display = 'none';
            });
        });

        loadEnrollmentMode();
        loadTokens();
    }

    // ── Public API ───────────────────────────────────────────────────────

    window.Tokens = {
        showCreateModal,
        showBulkModal,
        closeModal,
        closeBulkModal,
        closeCreatedModal,
        saveToken,
        editToken,
        revokeToken,
        generateBulk,
        copyToken,
    };

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();

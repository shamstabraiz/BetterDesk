/**
 * BetterDesk Console - Users Page
 * User management for admins
 */

(function() {
    'use strict';
    
    document.addEventListener('DOMContentLoaded', init);
    
    // State
    let users = [];
    let editingUserId = null;
    
    // Elements
    let tableBody, emptyState;
    
    function init() {
        tableBody = document.getElementById('users-tbody');
        emptyState = document.getElementById('users-empty');
        
        loadUsers();
        initEventListeners();
        
        window.addEventListener('app:refresh', loadUsers);
    }
    
    function initEventListeners() {
        // Add user button
        document.getElementById('add-user-btn')?.addEventListener('click', showAddUserModal);
    }
    
    /**
     * Load users from API
     */
    async function loadUsers() {
        try {
            const response = await Utils.api('/api/users');
            users = response.users || [];
            renderUsers();
        } catch (error) {
            console.error('Failed to load users:', error);
            if (error.status === 403) {
                Notifications.error(_('users.admin_only'));
            } else {
                Notifications.error(_('errors.load_users_failed'));
            }
        }
    }
    
    /**
     * Render users table
     */
    function renderUsers() {
        if (!tableBody) return;
        
        if (users.length === 0) {
            tableBody.innerHTML = '';
            emptyState?.classList.remove('hidden');
            return;
        }
        
        emptyState?.classList.add('hidden');
        
        tableBody.innerHTML = users.map(user => {
            const roleIcons = {
                super_admin: 'shield_person',
                admin: 'admin_panel_settings',
                server_admin: 'dns',
                global_admin: 'public',
                operator: 'engineering',
                viewer: 'visibility',
                pro: 'star'
            };
            const roleIcon = roleIcons[user.role] || 'person';
            const roleLabelKey = 'users.role_' + user.role;
            return `
            <tr data-id="${user.id}">
                <td>
                    <div class="user-info">
                        <div class="user-avatar">
                            <span class="material-icons">${roleIcon}</span>
                        </div>
                        <span class="user-username">${Utils.escapeHtml(user.username)}</span>
                    </div>
                </td>
                <td>
                    <span class="role-badge ${user.role}">
                        ${_(roleLabelKey)}
                    </span>
                </td>
                <td>${Utils.formatDate(user.created_at)}</td>
                <td>${user.last_login ? Utils.formatDate(user.last_login) : '<span class="text-muted">' + _('users.never') + '</span>'}</td>
                <td>
                    <div class="user-actions">
                        <button class="action-btn" title="${_('users.organizations')}" data-action="organizations" data-id="${user.id}" data-username="${Utils.escapeHtml(user.username)}">
                            <span class="material-icons">business</span>
                        </button>
                        <button class="action-btn" title="${_('users.reset_password')}" data-action="reset-password" data-id="${user.id}">
                            <span class="material-icons">lock_reset</span>
                        </button>
                        <button class="action-btn" title="${_('users.edit')}" data-action="edit" data-id="${user.id}">
                            <span class="material-icons">edit</span>
                        </button>
                        <button class="action-btn danger" title="${_('actions.delete')}" data-action="delete" data-id="${user.id}" data-username="${Utils.escapeHtml(user.username)}">
                            <span class="material-icons">delete</span>
                        </button>
                    </div>
                </td>
            </tr>
        `}).join('');        
        
        // Attach event listeners
        tableBody.querySelectorAll('.action-btn').forEach(btn => {
            btn.addEventListener('click', () => handleAction(btn.dataset.action, btn.dataset.id, btn.dataset));
        });
    }
    
    /**
     * Handle actions
     */
    async function handleAction(action, userId, data) {
        switch (action) {
            case 'edit':
                showEditUserModal(userId);
                break;
            case 'reset-password':
                await resetPassword(userId);
                break;
            case 'delete':
                await deleteUser(userId, data.username);
                break;
            case 'organizations':
                await showOrganizationsModal(userId, data.username);
                break;
        }
    }
    
    /**
     * Show add user modal
     */
    function showAddUserModal() {
        editingUserId = null;
        
        const template = document.getElementById('user-form-template');
        const content = template.content.cloneNode(true);
        const formHtml = content.querySelector('form').outerHTML;
        
        Modal.show({
            title: _('users.add_user'),
            content: formHtml,
            size: 'medium',
            buttons: [
                { label: _('actions.cancel'), class: 'btn-secondary', onClick: () => Modal.close() },
                { label: _('users.create'), class: 'btn-primary', onClick: () => submitUserForm() }
            ],
            onOpen: () => {
                initFormListeners();
                document.getElementById('user-username')?.focus();
            }
        });
    }
    
    /**
     * Show edit user modal
     */
    function showEditUserModal(userId) {
        const user = users.find(u => Number(u.id) === Number(userId));
        if (!user) return;
        
        editingUserId = user.id;
        
        const template = document.getElementById('user-form-template');
        const content = template.content.cloneNode(true);
        const formHtml = content.querySelector('form').outerHTML;
        
        Modal.show({
            title: _('users.edit_user'),
            content: formHtml,
            size: 'medium',
            buttons: [
                { label: _('actions.cancel'), class: 'btn-secondary', onClick: () => Modal.close() },
                { label: _('actions.save'), class: 'btn-primary', onClick: () => submitUserForm() }
            ],
            onOpen: () => {
                initFormListeners();
                
                // Fill form with user data
                const usernameInput = document.getElementById('user-username');
                const roleSelect = document.getElementById('user-role');
                const passwordInput = document.getElementById('user-password');
                
                if (usernameInput) {
                    usernameInput.value = user.username;
                    usernameInput.readOnly = true;
                    usernameInput.classList.add('readonly');
                }
                if (roleSelect) roleSelect.value = user.role;
                if (passwordInput) passwordInput.placeholder = _('users.password_leave_empty');
            }
        });
    }
    
    /**
     * Initialize form listeners
     */
    function initFormListeners() {
        // Password visibility toggle
        document.querySelector('.toggle-password')?.addEventListener('click', function() {
            const input = document.getElementById('user-password');
            const icon = this.querySelector('.material-icons');
            if (input.type === 'password') {
                input.type = 'text';
                icon.textContent = 'visibility_off';
            } else {
                input.type = 'password';
                icon.textContent = 'visibility';
            }
        });
        
        // Password strength indicator
        document.getElementById('user-password')?.addEventListener('input', function() {
            updatePasswordStrength(this.value);
        });
    }
    
    /**
     * Update password strength indicator
     */
    function updatePasswordStrength(password) {
        const container = document.getElementById('password-strength');
        if (!container) return;
        
        if (!password) {
            container.innerHTML = '';
            return;
        }
        
        let score = 0;
        const feedback = [];
        
        if (password.length >= 8) score++;
        else feedback.push(_('settings.req_length'));
        
        if (password.length >= 12) score++;
        
        if (/[a-z]/.test(password)) score++;
        else feedback.push(_('settings.req_lowercase'));
        
        if (/[A-Z]/.test(password)) score++;
        else feedback.push(_('settings.req_uppercase'));
        
        if (/[0-9]/.test(password)) score++;
        else feedback.push(_('settings.req_number'));
        
        if (/[^a-zA-Z0-9]/.test(password)) score++;
        
        const strength = score <= 2 ? 'weak' : score <= 4 ? 'medium' : 'strong';
        const labels = { weak: _('users.strength_weak'), medium: _('users.strength_medium'), strong: _('users.strength_strong') };
        
        container.innerHTML = `
            <div class="strength-bar">
                <div class="strength-fill ${strength}" style="width: ${(score / 6) * 100}%"></div>
            </div>
            <span class="strength-label ${strength}">${labels[strength]}</span>
        `;
    }
    
    /**
     * Submit user form
     */
    async function submitUserForm() {
        const form = document.getElementById('user-form');
        if (!form) return;
        
        const username = document.getElementById('user-username')?.value.trim();
        const password = document.getElementById('user-password')?.value;
        const role = document.getElementById('user-role')?.value;
        
        // Validate
        if (!editingUserId) {
            // Creating new user
            if (!username || !password) {
                Notifications.error(_('users.fill_required'));
                return;
            }
            
            if (!/^[a-zA-Z0-9_]{3,32}$/.test(username)) {
                Notifications.error(_('users.invalid_username'));
                return;
            }
            
            if (password.length < 8) {
                Notifications.error(_('users.password_too_short'));
                return;
            }
        }
        
        try {
            if (editingUserId) {
                // Update existing user
                const data = { role };
                if (password) data.password = password;
                
                await Utils.api(`/api/users/${editingUserId}`, {
                    method: 'PATCH',
                    body: data
                });
                Notifications.success(_('users.user_updated'));
            } else {
                // Create new user
                await Utils.api('/api/users', {
                    method: 'POST',
                    body: { username, password, role }
                });
                Notifications.success(_('users.user_created'));
            }
            
            Modal.close();
            loadUsers();
        } catch (error) {
            Notifications.error(error.message || _('errors.server_error'));
        }
    }
    
    /**
     * Reset user password
     */
    async function resetPassword(userId) {
        const user = users.find(u => Number(u.id) === Number(userId));
        if (!user) return;
        
        const newPassword = await Modal.prompt({
            title: _('users.reset_password'),
            label: _('users.new_password'),
            hint: _('users.password_hint'),
            inputType: 'password'
        });
        
        if (!newPassword) return;
        
        if (newPassword.length < 8) {
            Notifications.error(_('users.password_too_short'));
            return;
        }
        
        try {
            await Utils.api(`/api/users/${userId}/reset-password`, {
                method: 'POST',
                body: { newPassword }
            });
            Notifications.success(_('users.password_reset_success'));
        } catch (error) {
            Notifications.error(error.message || _('errors.server_error'));
        }
    }
    
    /**
     * Delete user
     */
    async function deleteUser(userId, username) {
        const confirmed = await Modal.confirm({
            title: _('users.delete_title'),
            message: _('users.delete_confirm', { username }),
            confirmLabel: _('actions.delete'),
            danger: true
        });
        
        if (!confirmed) return;
        
        try {
            await Utils.api(`/api/users/${userId}`, { method: 'DELETE' });
            Notifications.success(_('users.delete_success'));
            loadUsers();
        } catch (error) {
            Notifications.error(error.message || _('errors.server_error'));
        }
    }
    
    /**
     * Show organizations modal for user
     */
    async function showOrganizationsModal(userId, username) {
        let userOrgs = [];
        let allOrgs = [];
        
        try {
            // Fetch user's organizations
            const orgsResponse = await Utils.api(`/api/users/${userId}/organizations`);
            userOrgs = orgsResponse.organizations || [];
            
            // Fetch all organizations for adding
            const allOrgsResponse = await Utils.api('/api/panel/org');
            allOrgs = allOrgsResponse.organizations || [];
        } catch (error) {
            console.error('Failed to load organizations:', error);
            Notifications.error(_('errors.load_orgs_failed'));
            return;
        }
        
        // Filter out orgs user is already in
        const userOrgIds = new Set(userOrgs.map(o => String(o.org_id)));
        const availableOrgs = allOrgs.filter(o => !userOrgIds.has(String(o.id)));
        
        const orgsListHtml = userOrgs.length > 0 
            ? userOrgs.map(org => `
                <div class="org-assignment-item" data-org-id="${org.org_id}">
                    <div class="org-info">
                        <span class="material-icons">business</span>
                        <span class="org-name">${Utils.escapeHtml(org.org_name || org.name || 'Org #' + org.org_id)}</span>
                        <span class="role-badge ${org.role}">${_(org.role)}</span>
                    </div>
                    <button class="action-btn danger remove-org-btn" data-org-id="${org.org_id}" title="${_('actions.remove')}">
                        <span class="material-icons">remove_circle</span>
                    </button>
                </div>
            `).join('')
            : `<div class="empty-state-inline">${_('users.no_organizations')}</div>`;
        
        const addOrgHtml = availableOrgs.length > 0 
            ? `
                <div class="add-org-row">
                    <select id="add-org-select" class="form-input">
                        <option value="">${_('policies.select_org_placeholder')}</option>
                        ${availableOrgs.map(o => `<option value="${Utils.escapeHtml(String(o.id))}">${Utils.escapeHtml(o.name)}</option>`).join('')}
                    </select>
                    <select id="add-org-role" class="form-input" style="width: 120px;">
                        <option value="user">User</option>
                        <option value="operator">Operator</option>
                        <option value="admin">Admin</option>
                        <option value="owner">Owner</option>
                    </select>
                    <button id="add-org-btn" class="btn btn-primary btn-sm">
                        <span class="material-icons">add</span>
                        ${_('actions.add')}
                    </button>
                </div>
            `
            : '';
        
        Modal.show({
            title: _('users.user_organizations', { username }),
            content: `
                <div class="org-assignments">
                    <div class="org-assignments-list" id="user-orgs-list">
                        ${orgsListHtml}
                    </div>
                    ${addOrgHtml}
                </div>
            `,
            size: 'medium',
            buttons: [
                { label: _('actions.close'), class: 'btn-secondary', onClick: () => Modal.close() }
            ],
            onOpen: () => {
                // Remove org handler
                document.querySelectorAll('.remove-org-btn').forEach(btn => {
                    btn.addEventListener('click', async () => {
                        const orgId = btn.dataset.orgId;
                        try {
                            await Utils.api(`/api/panel/org/${orgId}/members/${userId}`, { method: 'DELETE' });
                            Notifications.success(_('users.org_removed'));
                            Modal.close();
                            showOrganizationsModal(userId, username); // Refresh
                        } catch (error) {
                            Notifications.error(error.message || _('errors.server_error'));
                        }
                    });
                });
                
                // Add org handler
                document.getElementById('add-org-btn')?.addEventListener('click', async () => {
                    const orgId = document.getElementById('add-org-select').value;
                    const role = document.getElementById('add-org-role').value;
                    
                    if (!orgId) {
                        Notifications.error(_('policies.select_org_placeholder'));
                        return;
                    }
                    
                    try {
                        await Utils.api(`/api/users/${userId}/organizations`, {
                            method: 'POST',
                            body: { org_id: orgId, role }
                        });
                        Notifications.success(_('organizations.user_linked'));
                        Modal.close();
                        showOrganizationsModal(userId, username); // Refresh
                    } catch (error) {
                        Notifications.error(error.message || _('errors.server_error'));
                    }
                });
            }
        });
    }
})();

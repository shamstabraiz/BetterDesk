/**
 * Yomie Console - Devices Page
 */

(function() {
    'use strict';
    
    // Fallback if Utils.sanitizeColor is missing (old utils.js on server)
    function _sanitizeColorFallback(c) {
        if (!c || typeof c !== 'string') return '#808080';
        if (/^#[0-9A-Fa-f]{3,6}$/.test(c)) return c;
        return '#808080';
    }

    // Map device_type to Material Icons
    function getDeviceTypeIcon(type) {
        switch ((type || '').toLowerCase()) {
            case 'yomie': return 'desktop_windows';
            case 'desktop':  return 'desktop_windows';
            case 'scada':    return 'precision_manufacturing';
            case 'iot':      return 'sensors';
            case 'os_agent': return 'terminal';
            case 'mobile':   return 'phone_android';
            case 'rustdesk': return 'connected_tv';
            default:         return 'devices';
        }
    }
    
    document.addEventListener('DOMContentLoaded', init);
    
    // State
    let devices = [];
    let filteredDevices = [];
    let folders = [];
    let selectedIds = new Set();
    let currentFilter = 'all';
    let currentFolder = 'all';
    let currentSort = { field: 'last_online', order: 'desc' };
    let currentPage = 1;
    let perPage = 20;
    let searchQuery = '';
    let draggedDeviceId = null;
    
    // Elements
    let tableBody, pagination, emptyState, bulkActions, selectedCountEl;
    
    function init() {
        // Cache elements
        tableBody = document.getElementById('devices-tbody');
        pagination = document.getElementById('pagination');
        emptyState = document.getElementById('devices-empty');
        bulkActions = document.getElementById('bulk-actions');
        selectedCountEl = document.getElementById('selected-count');
        
        // Load data
        loadFolders();
        loadDevices();
        
        // Event listeners
        initSearch();
        initFilters();
        initSorting();
        initSync();
        initFolders();
        initDragDrop();
        attachFolderDropEvents();  // For static folder chips
        initColumnVisibility();    // Column show/hide toggle
        initKebabGlobalClose();    // Close kebab menus on outside click
        
        // Refresh handler
        window.addEventListener('app:refresh', () => {
            loadFolders();
            loadDevices();
        });

        // Listen for changes from DeviceDetail panel
        document.addEventListener('deviceDetail:changed', () => {
            loadFolders();
            loadDevices();
        });

        // Real-time device status push via WebSocket
        initDeviceStatusWS();
    }

    /**
     * Connect to WebSocket for real-time device status updates.
     * Updates device status dots and badges without full page reload.
     */
    function initDeviceStatusWS() {
        const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${proto}//${location.host}/ws/device-status`;
        let ws = null;
        let retryDelay = 3000;

        function connect() {
            ws = new WebSocket(wsUrl);

            ws.onopen = () => {
                retryDelay = 3000;
            };

            ws.onmessage = (event) => {
                try {
                    const data = JSON.parse(event.data);
                    if (data.type === 'device_status') {
                        updateDeviceStatusInPlace(data.device_id, data.status);
                    }
                } catch (_) {}
            };

            ws.onclose = () => {
                setTimeout(connect, retryDelay);
                retryDelay = Math.min(retryDelay * 2, 60000);
            };

            ws.onerror = () => {
                ws.close();
            };
        }

        connect();
    }

    /**
     * Update a single device's status indicator without reloading the entire table.
     */
    function updateDeviceStatusInPlace(deviceId, status) {
        const row = tableBody?.querySelector(`tr[data-id="${deviceId}"]`);
        if (!row) return;

        const dot = row.querySelector('.device-status-dot');
        if (dot) {
            dot.className = 'device-status-dot';
            if (status === 'online' || status === 'ONLINE') {
                dot.classList.add('online');
                dot.title = 'Online';
            } else if (status === 'offline' || status === 'OFFLINE') {
                dot.classList.add('offline');
                dot.title = 'Offline';
            } else {
                dot.classList.add(status.toLowerCase());
                dot.title = status;
            }
        }

        // Also update the device in our local state
        const dev = allDevices.find(d => d.id === deviceId);
        if (dev) {
            dev.status = status;
            dev.live_status = status;
            dev.live_online = (status === 'online' || status === 'ONLINE');
        }
    }

    /**
     * Close all open kebab menus when clicking outside
     */
    function initKebabGlobalClose() {
        document.addEventListener('click', (e) => {
            if (!e.target.closest('.kebab-wrapper')) {
                closeAllKebabMenus();
            }
        });

        // Close on Escape key
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') closeAllKebabMenus();
        });

        // Close on scroll (position:fixed menus don't follow scroll)
        document.addEventListener('scroll', closeAllKebabMenus, true);
        window.addEventListener('resize', closeAllKebabMenus);
    }

    function closeAllKebabMenus() {
        document.querySelectorAll('.kebab-menu.open').forEach(m => {
            m.classList.remove('open');
            m.style.top = '';
            m.style.left = '';
        });
        const overlay = document.getElementById('kebab-overlay');
        if (overlay) overlay.classList.remove('open');
    }

    /**
     * Position a fixed kebab menu relative to its trigger button.
     * Flips upward if there is not enough space below.
     * Skips on mobile (≤600px) where CSS bottom-sheet handles positioning.
     */
    function positionKebabMenu(btn, menu) {
        if (window.innerWidth <= 600) return;

        // Force reflow so offsetHeight is accurate (menu just became display:block)
        menu.style.visibility = 'hidden';
        const prevDisplay = menu.style.display;
        menu.style.display = 'block';
        const menuHeight = menu.offsetHeight;
        const menuWidth = menu.offsetWidth || 180;
        menu.style.display = prevDisplay;
        menu.style.visibility = '';

        const rect = btn.getBoundingClientRect();
        const spaceBelow = window.innerHeight - rect.bottom;

        let top, left;

        // Vertical: prefer below, flip above if not enough space
        if (spaceBelow >= menuHeight + 8) {
            top = rect.bottom + 4;
        } else {
            top = rect.top - menuHeight - 4;
        }
        // Clamp to viewport bounds
        if (top < 8) top = 8;
        if (top + menuHeight > window.innerHeight - 8) {
            top = window.innerHeight - menuHeight - 8;
        }

        // Dynamically constrain max-height to available viewport space
        const availableHeight = window.innerHeight - top - 8;
        if (menuHeight > availableHeight) {
            menu.style.maxHeight = availableHeight + 'px';
        } else {
            menu.style.maxHeight = '';
        }

        // Horizontal: align right edge to button right edge
        left = rect.right - menuWidth;
        if (left < 8) left = 8;
        if (left + menuWidth > window.innerWidth - 8) {
            left = window.innerWidth - menuWidth - 8;
        }

        menu.style.top = top + 'px';
        menu.style.left = left + 'px';
    }
    
    /**
     * Load devices from API
     */
    async function loadDevices() {
        try {
            const response = await Utils.api('/api/devices');
            devices = response.devices || [];
            
            // Update count
            document.getElementById('devices-count').textContent = devices.length;
            
            // Update folder counts now that devices are loaded
            updateFolderCounts();
            
            applyFilters();
            
        } catch (error) {
            console.error('Failed to load devices:', error);
            Notifications.error(_('errors.load_devices_failed'));
        }
    }
    
    /**
     * Apply current filters and render
     */
    function applyFilters() {
        filteredDevices = devices.filter(device => {
            // Folder filter
            if (currentFolder === 'unassigned' && device.folder_id) return false;
            if (currentFolder !== 'all' && currentFolder !== 'unassigned') {
                if (device.folder_id !== parseInt(currentFolder, 10)) return false;
            }
            
            // Status filter
            if (currentFilter === 'online' && !device.online) return false;
            if (currentFilter === 'offline' && (device.online || device.banned)) return false;
            if (currentFilter === 'banned' && !device.banned) return false;
            
            // Search filter
            if (searchQuery) {
                const q = searchQuery.toLowerCase();
                const match = 
                    device.id?.toLowerCase().includes(q) ||
                    device.display_name?.toLowerCase().includes(q) ||
                    device.hostname?.toLowerCase().includes(q) ||
                    device.username?.toLowerCase().includes(q) ||
                    device.platform?.toLowerCase().includes(q) ||
                    (device.device_type || 'rustdesk').toLowerCase().includes(q);
                if (!match) return false;
            }
            
            return true;
        });
        
        // Sort
        sortDevices();
        
        // Render
        renderDevices();
        renderPagination();
        updateEmptyState();
    }
    
    /**
     * Sort devices
     */
    function sortDevices() {
        const { field, order } = currentSort;
        
        filteredDevices.sort((a, b) => {
            let valA = a[field];
            let valB = b[field];
            
            // Handle nulls
            if (valA === null || valA === undefined) valA = '';
            if (valB === null || valB === undefined) valB = '';
            
            // String comparison
            if (typeof valA === 'string') {
                valA = valA.toLowerCase();
                valB = valB.toLowerCase();
            }
            
            // Date comparison
            if (field === 'last_online') {
                valA = new Date(valA || 0).getTime();
                valB = new Date(valB || 0).getTime();
            }
            
            if (valA < valB) return order === 'asc' ? -1 : 1;
            if (valA > valB) return order === 'asc' ? 1 : -1;
            return 0;
        });
    }
    
    /**
     * Render devices table
     */
    function renderDevices() {
        if (!tableBody) return;
        
        const start = (currentPage - 1) * perPage;
        const end = start + perPage;
        const pageDevices = filteredDevices.slice(start, end);
        
        if (pageDevices.length === 0) {
            tableBody.innerHTML = '';
            return;
        }
        
        const statusClass = (d) => d.banned ? 'banned' : d.online ? 'online' : 'offline';
        const statusLabel = (d) => d.banned ? _('status.banned') : d.online ? _('status.online') : _('status.offline');

        tableBody.innerHTML = pageDevices.map(device => {
            const eid = Utils.escapeHtml(device.id);
            const sc = statusClass(device);
            return `
            <tr data-id="${eid}" class="${device.banned ? 'banned-row' : ''}" draggable="true">
                <td data-column="id">
                    <div class="device-id">
                        <span class="device-status-dot ${sc}"></span>
                        <span class="device-id-text">${eid}</span>
                        <button class="copy-btn" title="${_('actions.copy')}" data-copy="${eid}">
                            <span class="material-icons">content_copy</span>
                        </button>
                    </div>
                </td>
                <td data-column="hostname">${Utils.escapeHtml(device.display_name || device.hostname || device.note || '-')}</td>
                <td data-column="device_type">
                    <div class="platform-icon">
                        <span class="material-icons">${getDeviceTypeIcon(device.device_type)}</span>
                        <span>${Utils.escapeHtml(device.device_type || '-')}</span>
                    </div>
                </td>
                <td data-column="platform">
                    <div class="platform-icon">
                        <span class="material-icons">${Utils.getPlatformIcon(device.platform || device.os)}</span>
                        <span>${Utils.escapeHtml(device.platform || device.os || '-')}</span>
                    </div>
                </td>
                <td data-column="last_online">
                    <span class="last-seen-text" title="${Utils.formatDate(device.last_online)}">${Utils.formatRelativeTime(device.last_online)}</span>
                </td>
                <td data-column="status">
                    <span class="status-badge ${sc}"><span class="status-dot"></span>${statusLabel(device)}</span>
                </td>
                <td data-column="actions">
                    <div class="kebab-wrapper">
                        <button class="kebab-btn" title="${_('devices.actions')}">
                            <span class="material-icons">more_vert</span>
                        </button>
                        <div class="kebab-menu">
                            <button class="kebab-menu-item connect-desktop" data-action="web-remote" data-id="${eid}">
                                <span class="material-icons">screen_share</span>
                                <span>${_('actions.web_remote') || 'Web Remote'}</span>
                            </button>
                            <button class="kebab-menu-item" data-action="cdap-viewer" data-id="${eid}">
                                <span class="material-icons">photo_camera</span>
                                <span>${_('actions.cdap_viewer') || 'CDAP Snapshot Viewer'}</span>
                            </button>
                            <button class="kebab-menu-item" data-action="connect-desktop" data-id="${eid}">
                                <span class="material-icons">computer</span>
                                <span>${_('actions.connect_desktop') || 'Desktop Client'}</span>
                            </button>
                            <div class="kebab-divider"></div>
                            <button class="kebab-menu-item info" data-action="details" data-id="${eid}">
                                <span class="material-icons">info</span>
                                <span>${_('actions.details')}</span>
                            </button>
                            <button class="kebab-menu-item" data-action="edit" data-id="${eid}">
                                <span class="material-icons">edit</span>
                                <span>${_('actions.edit')}</span>
                            </button>
                            <button class="kebab-menu-item" data-action="access-policy" data-id="${eid}">
                                <span class="material-icons">lock</span>
                                <span>${_('devices.access_policy') || 'Access Policy'}</span>
                            </button>
                            <button class="kebab-menu-item ${device.banned ? 'unban' : 'ban'}" data-action="toggle-ban" data-id="${eid}" data-banned="${device.banned}">
                                <span class="material-icons">${device.banned ? 'check_circle' : 'block'}</span>
                                <span>${device.banned ? _('actions.unban') : _('actions.ban')}</span>
                            </button>
                            <div class="kebab-divider"></div>
                            <button class="kebab-menu-item danger" data-action="delete" data-id="${eid}">
                                <span class="material-icons">delete</span>
                                <span>${_('actions.delete')}</span>
                            </button>
                        </div>
                    </div>
                </td>
            </tr>`;
        }).join('');
        
        // Re-apply column visibility to newly rendered rows
        applyColumnVisibility();
        
        // Attach event listeners
        attachRowEventListeners();
    }
    
    /**
     * Attach event listeners to table rows
     */
    function attachRowEventListeners() {
        // Copy ID
        tableBody.querySelectorAll('.copy-btn').forEach(btn => {
            btn.addEventListener('click', async (e) => {
                e.stopPropagation();
                const id = btn.dataset.copy;
                await Utils.copyToClipboard(id);
                btn.classList.add('copied');
                setTimeout(() => btn.classList.remove('copied'), 2000);
                Notifications.success(_('common.copied'));
            });
        });

        // Kebab menu toggle
        tableBody.querySelectorAll('.kebab-btn').forEach(btn => {
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                const menu = btn.nextElementSibling;
                const wasOpen = menu.classList.contains('open');
                closeAllKebabMenus();
                if (!wasOpen) {
                    menu.classList.add('open');
                    positionKebabMenu(btn, menu);
                    const overlay = document.getElementById('kebab-overlay');
                    if (overlay) overlay.classList.add('open');
                }
            });
        });

        // Kebab menu item actions
        tableBody.querySelectorAll('.kebab-menu-item').forEach(btn => {
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                closeAllKebabMenus();
                handleAction(btn.dataset.action, btn.dataset.id, btn.dataset);
            });
        });

        // Double-click row to open device detail panel
        tableBody.querySelectorAll('tr[data-id]').forEach(row => {
            row.addEventListener('dblclick', (e) => {
                // Ignore double-click on kebab menu and copy button
                if (e.target.closest('.kebab-wrapper') || e.target.closest('.copy-btn')) return;
                const deviceId = row.dataset.id;
                if (deviceId && typeof DeviceDetail !== 'undefined') {
                    DeviceDetail.open(deviceId);
                }
            });
        });
    }
    
    /**
     * Handle device actions
     */
    /**
     * Try to add a remote session tab in an existing RDClient window via BroadcastChannel.
     * Falls back to opening a new browser tab if no RDClient page is listening.
     */
    function _tryAddRemoteTab(deviceId, data) {
        if (typeof BroadcastChannel === 'undefined') {
            window.open(`/remote/${encodeURIComponent(deviceId)}`, '_blank');
            return;
        }

        const bc = new BroadcastChannel('yomie-remote');
        let handled = false;

        // Listen for acknowledgment from remote page
        bc.onmessage = (ev) => {
            if (ev.data && ev.data.type === 'pong') {
                // Remote page exists — send the add-session command
                handled = true;
                bc.postMessage({
                    type: 'add-session',
                    deviceId: deviceId,
                    deviceName: (data && data.deviceName) || ''
                });
                bc.close();
            } else if (ev.data && ev.data.type === 'session-added') {
                handled = true;
                bc.close();
            }
        };

        // Ping to check if a remote page is open
        bc.postMessage({ type: 'ping' });

        // Timeout: if no response within 300ms, open new tab
        setTimeout(() => {
            if (!handled) {
                bc.close();
                window.open(`/remote/${encodeURIComponent(deviceId)}`, '_blank');
            }
        }, 300);
    }

    async function handleAction(action, deviceId, data) {
        switch (action) {
            case 'web-remote':
                _tryAddRemoteTab(deviceId, data);
                break;

            case 'cdap-viewer':
                window.open(`/remote-cdap/${encodeURIComponent(deviceId)}`, '_blank');
                break;

            case 'connect-desktop':
                connectDesktopClient(deviceId);
                break;

            case 'remote-viewer':
                // Legacy action: route through the unified web remote client.
                _tryAddRemoteTab(deviceId, data);
                break;

            case 'details':
                if (typeof DeviceDetail !== 'undefined') {
                    DeviceDetail.open(deviceId);
                } else {
                    showDeviceDetails(deviceId);
                }
                break;
                
            case 'edit':
                showEditModal(deviceId);
                break;
                
            case 'toggle-ban':
                await toggleBan(deviceId, data.banned === 'true');
                break;
                
            case 'change-id':
                await changeDeviceId(deviceId);
                break;
                
            case 'delete':
                await deleteDevice(deviceId);
                break;

            case 'access-policy':
                showAccessPolicyModal(deviceId);
                break;
        }
    }
    
    /**
     * Connect to device via RustDesk desktop client (rustdesk:// protocol)
     */
    function connectDesktopClient(deviceId) {
        window.open('rustdesk://' + encodeURIComponent(deviceId), '_blank');
    }
    
    /**
     * Show device details modal
     */
    async function showDeviceDetails(deviceId) {
        try {
            const device = await Utils.api(`/api/devices/${deviceId}`);
            
            const content = `
                <div class="device-details">
                    <div class="detail-row">
                        <span class="detail-label">${_('devices.id')}:</span>
                        <span class="detail-value"><strong>${Utils.escapeHtml(device.id)}</strong></span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">${_('devices.hostname')}:</span>
                        <span class="detail-value">${Utils.escapeHtml(device.display_name || device.hostname || device.note || '-')}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">${_('devices.platform')}:</span>
                        <span class="detail-value">${Utils.escapeHtml(device.platform || '-')}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">${_('status.label')}:</span>
                        <span class="detail-value">${device.banned ? _('status.banned') : device.online ? _('status.online') : _('status.offline')}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">${_('devices.last_online')}:</span>
                        <span class="detail-value">${Utils.formatDate(device.last_online)}</span>
                    </div>
                    <div class="detail-row">
                        <span class="detail-label">${_('devices.created')}:</span>
                        <span class="detail-value">${Utils.formatDate(device.created_at)}</span>
                    </div>
                    ${device.ban_reason ? `
                    <div class="detail-row">
                        <span class="detail-label">${_('devices.ban_reason')}:</span>
                        <span class="detail-value">${Utils.escapeHtml(device.ban_reason)}</span>
                    </div>
                    ` : ''}
                </div>
            `;
            
            Modal.show({
                title: _('devices.details'),
                content: content,
                size: 'medium',
                buttons: [
                    { label: _('actions.ok'), class: 'btn-primary', onClick: () => Modal.close() }
                ]
            });
        } catch (error) {
            Notifications.error(error.message || _('errors.load_device_failed'));
        }
    }

    /**
     * Show Access Policy modal for unattended access management
     */
    async function showAccessPolicyModal(deviceId) {
        try {
            const resp = await Utils.api(`/api/devices/${deviceId}/access-policy`);
            const policy = resp.data || resp;

            const days = ['mon', 'tue', 'wed', 'thu', 'fri', 'sat', 'sun'];
            const activeDays = (policy.schedule_days || '').split(',').map(d => d.trim().toLowerCase()).filter(Boolean);

            const dayCheckboxes = days.map(d => {
                const checked = activeDays.includes(d) ? 'checked' : '';
                const label = _('days.' + d) || d.charAt(0).toUpperCase() + d.slice(1);
                return `<label class="day-checkbox"><input type="checkbox" name="schedule_day" value="${d}" ${checked}> ${label}</label>`;
            }).join('');

            const content = `
                <div class="access-policy-form">
                    <div class="form-section">
                        <h4><span class="material-icons">vpn_key</span> ${_('devices.unattended_access') || 'Unattended Access'}</h4>
                        <label class="toggle-row">
                            <input type="checkbox" id="ap-unattended" ${policy.unattended_enabled ? 'checked' : ''}>
                            <span>${_('devices.enable_unattended') || 'Enable unattended access'}</span>
                        </label>
                    </div>
                    <div class="form-section">
                        <h4><span class="material-icons">lock</span> ${_('devices.access_password') || 'Access Password'}</h4>
                        <p class="form-hint">${policy.password_set ? '<span class="badge badge-success">✓ ' + (_('devices.password_configured') || 'Password set') + '</span>' : '<span class="badge badge-warning">' + (_('devices.no_password') || 'No password') + '</span>'}</p>
                        <div class="form-row">
                            <input type="password" id="ap-password" class="form-input" placeholder="${_('devices.new_password') || 'New password (leave empty to keep current)'}">
                        </div>
                        <label class="toggle-row">
                            <input type="checkbox" id="ap-clear-password">
                            <span>${_('devices.clear_password') || 'Clear password'}</span>
                        </label>
                    </div>
                    <div class="form-section">
                        <h4><span class="material-icons">schedule</span> ${_('devices.access_schedule') || 'Access Schedule'}</h4>
                        <label class="toggle-row">
                            <input type="checkbox" id="ap-schedule" ${policy.schedule_enabled ? 'checked' : ''}>
                            <span>${_('devices.enable_schedule') || 'Enable time-based access'}</span>
                        </label>
                        <div id="ap-schedule-fields" style="${policy.schedule_enabled ? '' : 'opacity:0.5;pointer-events:none'}">
                            <div class="form-row">
                                <label>${_('devices.allowed_days') || 'Allowed days'}:</label>
                                <div class="day-checkboxes">${dayCheckboxes}</div>
                            </div>
                            <div class="form-row form-row-inline">
                                <div>
                                    <label>${_('devices.start_time') || 'Start time'}:</label>
                                    <input type="time" id="ap-start-time" class="form-input" value="${policy.schedule_start_time || '08:00'}">
                                </div>
                                <div>
                                    <label>${_('devices.end_time') || 'End time'}:</label>
                                    <input type="time" id="ap-end-time" class="form-input" value="${policy.schedule_end_time || '18:00'}">
                                </div>
                            </div>
                            <div class="form-row">
                                <label>${_('devices.timezone') || 'Timezone'}:</label>
                                <input type="text" id="ap-timezone" class="form-input" placeholder="Europe/Warsaw" value="${Utils.escapeHtml(policy.schedule_timezone || Intl.DateTimeFormat().resolvedOptions().timeZone)}">
                            </div>
                        </div>
                    </div>
                    <div class="form-section">
                        <h4><span class="material-icons">people</span> ${_('devices.allowed_operators') || 'Allowed Operators'}</h4>
                        <div class="form-row">
                            <input type="text" id="ap-operators" class="form-input" placeholder="${_('devices.all_operators') || 'All operators (leave empty)'}" value="${Utils.escapeHtml(policy.allowed_operators || '')}">
                            <p class="form-hint">${_('devices.operators_hint') || 'Comma-separated usernames. Leave empty to allow all operators.'}</p>
                        </div>
                    </div>
                    ${policy.updated_by ? `<p class="form-meta">${_('devices.last_updated_by') || 'Last updated by'}: ${Utils.escapeHtml(policy.updated_by)} ${policy.updated_at ? '(' + Utils.formatDate(policy.updated_at) + ')' : ''}</p>` : ''}
                </div>
            `;

            Modal.show({
                title: (_('devices.access_policy') || 'Access Policy') + ' — ' + deviceId,
                content: content,
                size: 'large',
                buttons: [
                    { label: _('actions.cancel'), class: 'btn-secondary', onClick: () => Modal.close() },
                    {
                        label: _('actions.save'), class: 'btn-primary', onClick: async () => {
                            const scheduleCheckbox = document.getElementById('ap-schedule');
                            const selectedDays = Array.from(document.querySelectorAll('input[name="schedule_day"]:checked')).map(cb => cb.value);
                            const payload = {
                                unattended_enabled: document.getElementById('ap-unattended').checked,
                                password: document.getElementById('ap-password').value,
                                clear_password: document.getElementById('ap-clear-password').checked,
                                schedule_enabled: scheduleCheckbox ? scheduleCheckbox.checked : false,
                                schedule_days: selectedDays.join(','),
                                schedule_start_time: document.getElementById('ap-start-time') ? document.getElementById('ap-start-time').value : '',
                                schedule_end_time: document.getElementById('ap-end-time') ? document.getElementById('ap-end-time').value : '',
                                schedule_timezone: document.getElementById('ap-timezone') ? document.getElementById('ap-timezone').value : '',
                                allowed_operators: document.getElementById('ap-operators') ? document.getElementById('ap-operators').value : ''
                            };

                            try {
                                await Utils.api(`/api/devices/${deviceId}/access-policy`, {
                                    method: 'PUT',
                                    headers: { 'Content-Type': 'application/json' },
                                    body: JSON.stringify(payload)
                                });
                                Notifications.success(_('devices.access_policy_saved') || 'Access policy saved');
                                Modal.close();
                            } catch (err) {
                                Notifications.error(err.message || 'Failed to save access policy');
                            }
                        }
                    }
                ]
            });

            // Toggle schedule fields on checkbox change
            const scheduleChk = document.getElementById('ap-schedule');
            const scheduleFields = document.getElementById('ap-schedule-fields');
            if (scheduleChk && scheduleFields) {
                scheduleChk.addEventListener('change', () => {
                    scheduleFields.style.opacity = scheduleChk.checked ? '1' : '0.5';
                    scheduleFields.style.pointerEvents = scheduleChk.checked ? 'auto' : 'none';
                });
            }

        } catch (error) {
            Notifications.error(error.message || 'Failed to load access policy');
        }
    }

    /**
     * Toggle device ban status
     */
    async function toggleBan(deviceId, currentlyBanned) {
        const action = currentlyBanned ? 'unban' : 'ban';
        const confirmed = await Modal.confirm({
            title: _(`devices.${action}_title`),
            message: _(`devices.${action}_confirm`, { id: deviceId }),
            confirmLabel: _(currentlyBanned ? 'actions.unban' : 'actions.ban'),
            danger: !currentlyBanned
        });
        
        if (!confirmed) return;
        
        try {
            await Utils.api(`/api/devices/${deviceId}/${action}`, { method: 'POST' });
            Notifications.success(_(`devices.${action}_success`));
            loadDevices();
        } catch (error) {
            Notifications.error(error.message || _(`errors.${action}_failed`));
        }
    }
    
    /**
     * Change device ID
     */
    async function changeDeviceId(deviceId) {
        const newId = await Modal.prompt({
            title: _('devices.change_id_title'),
            label: _('devices.new_id'),
            placeholder: 'NEWID123',
            hint: _('devices.change_id_hint')
        });
        
        if (!newId) return;
        
        // Validate
        if (newId.length < 6 || newId.length > 16) {
            Notifications.error(_('devices.id_length_error'));
            return;
        }
        
        if (!/^[A-Z0-9_-]+$/i.test(newId)) {
            Notifications.error(_('devices.id_format_error'));
            return;
        }
        
        try {
            await Utils.api(`/api/devices/${deviceId}/change-id`, {
                method: 'POST',
                body: { newId: newId.toUpperCase() }
            });
            Notifications.success(_('devices.change_id_success'));
            loadDevices();
        } catch (error) {
            Notifications.error(error.message || _('errors.change_id_failed'));
        }
    }
    
    /**
     * Delete device with delayed confirmation
     */
    async function deleteDevice(deviceId) {
        return new Promise((resolve) => {
            const modalHtml = `
                <div class="modal-overlay delete-confirm-modal" id="delete-modal-${deviceId}">
                    <div class="modal-container modal-danger">
                        <div class="modal-header">
                            <h3 class="modal-title">
                                <span class="material-icons" style="color: var(--accent-red);">warning</span>
                                ${_('devices.delete_title')}
                            </h3>
                        </div>
                        <div class="modal-body">
                            <p class="delete-warning">${_('devices.delete_warning')}</p>
                            <p class="delete-device-id"><strong>${Utils.escapeHtml(deviceId)}</strong></p>
                            <p class="delete-info">${_('devices.delete_permanent')}</p>
                            <div class="revoke-options" style="margin-top: 12px; padding: 10px; border: 1px solid var(--border-color); border-radius: 6px;">
                                <label class="checkbox-label" style="display: flex; align-items: center; gap: 8px; cursor: pointer; margin-bottom: 6px;">
                                    <input type="checkbox" id="revoke-check-${deviceId}" />
                                    <span class="material-icons" style="font-size: 18px; color: var(--accent-red);">block</span>
                                    <span>${_('devices.revoke_option')}</span>
                                </label>
                                <p class="revoke-hint" style="font-size: 0.8rem; opacity: 0.7; margin: 0 0 0 30px;">
                                    ${_('devices.revoke_hint')}
                                </p>
                            </div>
                        </div>
                        <div class="modal-footer">
                            <button class="btn btn-secondary cancel-btn">${_('actions.cancel')}</button>
                            <button class="btn btn-danger confirm-delete-btn" disabled>
                                <span class="material-icons">delete_forever</span>
                                <span class="btn-text">${_('actions.delete')} (<span class="countdown">3</span>)</span>
                            </button>
                        </div>
                    </div>
                </div>
            `;
            
            document.body.insertAdjacentHTML('beforeend', modalHtml);
            
            const modal = document.getElementById(`delete-modal-${deviceId}`);
            const confirmBtn = modal.querySelector('.confirm-delete-btn');
            const cancelBtn = modal.querySelector('.cancel-btn');
            const countdownEl = confirmBtn.querySelector('.countdown');
            const revokeCheck = document.getElementById(`revoke-check-${deviceId}`);
            
            let countdown = 3;
            const timer = setInterval(() => {
                countdown--;
                if (countdownEl) {
                    countdownEl.textContent = countdown;
                }
                if (countdown <= 0) {
                    clearInterval(timer);
                    confirmBtn.disabled = false;
                    const btnText = confirmBtn.querySelector('.btn-text');
                    if (btnText) btnText.textContent = _('actions.delete');
                }
            }, 1000);
            
            const closeModal = () => {
                clearInterval(timer);
                modal.remove();
            };
            
            cancelBtn.addEventListener('click', () => {
                closeModal();
                resolve(false);
            });
            
            modal.addEventListener('click', (e) => {
                if (e.target === modal) {
                    closeModal();
                    resolve(false);
                }
            });
            
            confirmBtn.addEventListener('click', async () => {
                if (confirmBtn.disabled) return;
                const revoke = revokeCheck && revokeCheck.checked;
                closeModal();
                
                try {
                    const params = new URLSearchParams();
                    if (revoke) params.set('revoke', 'true');
                    const qs = params.toString();
                    const url = `/api/devices/${deviceId}${qs ? '?' + qs : ''}`;
                    await Utils.api(url, { method: 'DELETE' });
                    const msg = revoke ? _('devices.revoke_success') : _('devices.delete_success');
                    Notifications.success(msg);
                    loadDevices();
                    resolve(true);
                } catch (error) {
                    Notifications.error(error.message || _('errors.delete_failed'));
                    resolve(false);
                }
            });
        });
    }
    
    /**
     * Show edit modal
     */
    function showEditModal(deviceId) {
        const device = devices.find(d => d.id === deviceId);
        if (!device) return;
        
        Modal.show({
            title: _('devices.edit_title'),
            content: `
                <div class="device-info-grid">
                    <div class="device-info-item">
                        <label>${_('devices.id')}</label>
                        <span>${Utils.escapeHtml(device.id)}</span>
                    </div>
                    <div class="device-info-item">
                        <label>${_('devices.hostname')}</label>
                        <span>${Utils.escapeHtml(device.hostname || '-')}</span>
                    </div>
                    <div class="device-info-item full-width">
                        <label for="edit-display-name">${_('devices.display_name')}</label>
                        <input type="text" id="edit-display-name" class="form-input" 
                               value="${Utils.escapeHtml(device.display_name || '')}" 
                               placeholder="${_('devices.display_name_placeholder')}" maxlength="128" />
                    </div>
                    <div class="device-info-item full-width">
                        <label for="edit-note">${_('devices.note')}</label>
                        <textarea id="edit-note" class="form-input" rows="2" 
                                  placeholder="${_('devices.note_placeholder')}" maxlength="512">${Utils.escapeHtml(device.note || '')}</textarea>
                    </div>
                    <div class="device-info-item">
                        <label>${_('devices.username')}</label>
                        <span>${Utils.escapeHtml(device.username || '-')}</span>
                    </div>
                    <div class="device-info-item">
                        <label>${_('devices.platform')}</label>
                        <span>${Utils.escapeHtml(device.platform || '-')}</span>
                    </div>
                    <div class="device-info-item">
                        <label>${_('devices.version')}</label>
                        <span>${Utils.escapeHtml(device.version || '-')}</span>
                    </div>
                    <div class="device-info-item">
                        <label>${_('devices.first_seen')}</label>
                        <span>${Utils.formatDate(device.created_at)}</span>
                    </div>
                    <div class="device-info-item">
                        <label>${_('devices.last_seen')}</label>
                        <span>${Utils.formatDate(device.last_online)}</span>
                    </div>
                </div>
            `,
            buttons: [
                { label: _('actions.save'), class: 'btn-primary', onClick: async () => {
                    const displayName = document.getElementById('edit-display-name').value.trim();
                    const note = document.getElementById('edit-note').value.trim();
                    const saveBtn = document.querySelector('.modal-overlay.open [data-btn-index="0"]');
                    const closeBtn = document.querySelector('.modal-overlay.open [data-btn-index="1"]');
                    const restoreButtons = () => {
                        if (saveBtn) {
                            saveBtn.disabled = false;
                            saveBtn.textContent = _('actions.save');
                        }
                        if (closeBtn) closeBtn.disabled = false;
                    };

                    if (saveBtn) {
                        saveBtn.disabled = true;
                        saveBtn.textContent = _('common.loading');
                    }
                    if (closeBtn) closeBtn.disabled = true;

                    try {
                        const resp = await Utils.api(`/api/devices/${encodeURIComponent(device.id)}`, {
                            method: 'PATCH',
                            body: JSON.stringify({ display_name: displayName, note }),
                            headers: { 'Content-Type': 'application/json' }
                        });
                        if (resp.success) {
                            device.display_name = displayName;
                            device.note = note;
                            applyFilters();
                            document.dispatchEvent(new CustomEvent('devices:updated', {
                                detail: {
                                    id: device.id,
                                    display_name: displayName,
                                    note: note
                                }
                            }));

                            if (saveBtn) {
                                saveBtn.textContent = _('common.saved');
                            }

                            Notifications.success(_('common.saved'), _('devices.display_name'));
                            setTimeout(() => Modal.close(), 250);
                            loadDevices();
                        } else {
                            restoreButtons();
                            Notifications.error(resp.error || _('errors.server_error'));
                        }
                    } catch (err) {
                        restoreButtons();
                        Notifications.error(err.message || _('errors.server_error'));
                    }
                }},
                { label: _('actions.close'), class: 'btn-secondary', onClick: () => Modal.close() }
            ],
            size: 'medium'
        });
    }

    window.YomieDevices = window.YomieDevices || {};
    window.YomieDevices.showEditModal = showEditModal;
    
    /**
     * Render pagination
     */
    function renderPagination() {
        const totalPages = Math.ceil(filteredDevices.length / perPage);
        const paginationInfo = document.getElementById('pagination-info');
        const paginationControls = document.getElementById('pagination-controls');
        
        // Update info
        const start = Math.min((currentPage - 1) * perPage + 1, filteredDevices.length);
        const end = Math.min(currentPage * perPage, filteredDevices.length);
        paginationInfo.textContent = `${_('devices.showing')} ${start}-${end} ${_('devices.of')} ${filteredDevices.length}`;
        
        // Generate controls
        let html = '';
        
        html += `<button class="pagination-btn" ${currentPage === 1 ? 'disabled' : ''} data-page="${currentPage - 1}">
            <span class="material-icons">chevron_left</span>
        </button>`;
        
        // Page numbers
        for (let i = 1; i <= totalPages; i++) {
            if (i === 1 || i === totalPages || (i >= currentPage - 1 && i <= currentPage + 1)) {
                html += `<button class="pagination-btn ${i === currentPage ? 'active' : ''}" data-page="${i}">${i}</button>`;
            } else if (i === currentPage - 2 || i === currentPage + 2) {
                html += `<span style="padding: 0 4px;">...</span>`;
            }
        }
        
        html += `<button class="pagination-btn" ${currentPage === totalPages ? 'disabled' : ''} data-page="${currentPage + 1}">
            <span class="material-icons">chevron_right</span>
        </button>`;
        
        paginationControls.innerHTML = html;
        
        // Event listeners
        paginationControls.querySelectorAll('.pagination-btn').forEach(btn => {
            btn.addEventListener('click', () => {
                const page = parseInt(btn.dataset.page);
                if (page && page !== currentPage && page >= 1 && page <= totalPages) {
                    currentPage = page;
                    renderDevices();
                    renderPagination();
                }
            });
        });
    }
    
    /**
     * Update empty state
     */
    function updateEmptyState() {
        const tableContainer = document.querySelector('.devices-table-container');
        
        if (filteredDevices.length === 0) {
            tableContainer.classList.add('hidden');
            emptyState.classList.remove('hidden');
        } else {
            tableContainer.classList.remove('hidden');
            emptyState.classList.add('hidden');
        }
    }
    
    /**
     * Initialize search
     */
    function initSearch() {
        const searchInput = document.getElementById('search-input');
        if (!searchInput) return;
        
        searchInput.addEventListener('input', Utils.debounce((e) => {
            searchQuery = e.target.value.trim();
            currentPage = 1;
            applyFilters();
        }, 300));
    }
    
    /**
     * Initialize filters
     */
    function initFilters() {
        document.querySelectorAll('.filter-btn').forEach(btn => {
            btn.addEventListener('click', () => {
                document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
                btn.classList.add('active');
                currentFilter = btn.dataset.filter;
                currentPage = 1;
                applyFilters();
            });
        });
    }
    
    /**
     * Initialize sorting
     */
    function initSorting() {
        document.querySelectorAll('th.sortable').forEach(th => {
            th.addEventListener('click', () => {
                const field = th.dataset.sort;
                if (currentSort.field === field) {
                    currentSort.order = currentSort.order === 'asc' ? 'desc' : 'asc';
                } else {
                    currentSort.field = field;
                    currentSort.order = 'asc';
                }
                applyFilters();
            });
        });
    }
    
    /**
     * Initialize selection
     */
    function initSelection() {
        const selectAll = document.getElementById('select-all');
        if (!selectAll) return;
        
        selectAll.addEventListener('change', () => {
            const checkboxes = tableBody.querySelectorAll('.device-checkbox');
            checkboxes.forEach(cb => {
                cb.checked = selectAll.checked;
                const id = cb.dataset.id;
                if (selectAll.checked) {
                    selectedIds.add(id);
                } else {
                    selectedIds.delete(id);
                }
            });
            updateSelectionUI();
        });
    }
    
    /**
     * Update selection UI
     */
    function updateSelectionUI() {
        if (selectedIds.size > 0) {
            bulkActions.classList.remove('hidden');
            selectedCountEl.textContent = selectedIds.size;
        } else {
            bulkActions.classList.add('hidden');
        }
        
        // Update select all checkbox
        const selectAll = document.getElementById('select-all');
        const checkboxes = tableBody.querySelectorAll('.device-checkbox');
        selectAll.checked = checkboxes.length > 0 && selectedIds.size === checkboxes.length;
        selectAll.indeterminate = selectedIds.size > 0 && selectedIds.size < checkboxes.length;
    }
    
    /**
     * Initialize bulk actions
     */
    function initBulkActions() {
        document.getElementById('clear-selection')?.addEventListener('click', () => {
            selectedIds.clear();
            tableBody.querySelectorAll('.device-checkbox').forEach(cb => cb.checked = false);
            document.getElementById('select-all').checked = false;
            updateSelectionUI();
        });
        
        document.getElementById('bulk-delete')?.addEventListener('click', async () => {
            const count = selectedIds.size;
            const confirmed = await Modal.confirm({
                title: _('devices.bulk_delete_title'),
                message: _('devices.bulk_delete_confirm', { count }),
                confirmLabel: _('actions.delete'),
                confirmIcon: 'delete',
                danger: true
            });
            
            if (!confirmed) return;
            
            try {
                await Utils.api('/api/devices/bulk-delete', {
                    method: 'POST',
                    body: { ids: Array.from(selectedIds) }
                });
                Notifications.success(_('devices.bulk_delete_success', { count }));
                selectedIds.clear();
                loadDevices();
            } catch (error) {
                Notifications.error(error.message || _('errors.bulk_delete_failed'));
            }
        });
    }
    
    /**
     * Initialize sync button
     */
    function initSync() {
        document.getElementById('sync-btn')?.addEventListener('click', async () => {
            try {
                await Utils.api('/api/sync-status', { method: 'POST' });
                Notifications.success(_('devices.sync_success'));
                loadDevices();
            } catch (error) {
                Notifications.error(error.message || _('errors.sync_failed'));
            }
        });
    }
    
    // ==================== Folder Functions ====================
    
    /**
     * Load folders from API
     */
    async function loadFolders() {
        try {
            const response = await Utils.api('/api/folders');
            folders = response.folders || [];
            // Expose folders globally for DeviceDetail panel
            window._yomie_folders = folders;
            renderFolders();
            updateBulkMoveSelect();
        } catch (error) {
            console.error('Failed to load folders:', error);
        }
    }
    
    /**
     * Render folders list
     */
    function renderFolders() {
        const container = document.getElementById('custom-folders');
        if (!container) return;
        
        if (folders.length === 0) {
            container.innerHTML = '';
            attachFolderDropEvents();
            return;
        }
        
        container.innerHTML = folders.map(folder => {
            const safeColor = (Utils.sanitizeColor || _sanitizeColorFallback)(folder.color);
            return `
            <button class="folder-chip ${currentFolder == folder.id ? 'active' : ''}" 
                 data-folder="${folder.id}" 
                 style="--folder-color: ${safeColor}">
                <span class="material-icons chip-icon" style="color: ${safeColor}">folder</span>
                <span class="chip-label">${Utils.escapeHtml(folder.name)}</span>
                <span class="chip-count">${folder.device_count || 0}</span>
                <span class="chip-actions">
                    <span class="chip-action folder-edit" data-id="${folder.id}" title="${_('actions.edit')}">
                        <span class="material-icons">edit</span>
                    </span>
                    <span class="chip-action folder-delete" data-id="${folder.id}" title="${_('actions.delete')}">
                        <span class="material-icons">delete</span>
                    </span>
                </span>
            </button>
        `}).join('');
        
        // Attach folder click listeners
        container.querySelectorAll('.folder-chip').forEach(el => {
            el.addEventListener('click', (e) => {
                if (!e.target.closest('.chip-actions')) {
                    selectFolder(el.dataset.folder);
                }
            });
        });
        
        container.querySelectorAll('.folder-edit').forEach(btn => {
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                editFolder(btn.dataset.id);
            });
        });
        
        container.querySelectorAll('.folder-delete').forEach(btn => {
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                deleteFolder(btn.dataset.id);
            });
        });
        
        // Attach drag & drop events for all folder chips
        attachFolderDropEvents();
    }
    
    /**
     * Update folder counts
     */
    function updateFolderCounts() {
        // All devices count
        const allCount = document.getElementById('folder-count-all');
        if (allCount) allCount.textContent = devices.length;
        
        // Unassigned count (devices without folder_id — null, undefined, or missing)
        const unassignedCount = document.getElementById('folder-count-unassigned');
        if (unassignedCount) {
            const count = devices.filter(d => !d.folder_id).length;
            unassignedCount.textContent = count;
        }
        
        // Update custom folder counts from devices array
        for (const folder of folders) {
            const el = document.querySelector(`.folder-chip[data-folder="${folder.id}"] .chip-count`);
            if (el) {
                const count = devices.filter(d => d.folder_id === folder.id).length;
                el.textContent = count;
            }
        }
    }
    
    /**
     * Update bulk move select options
     */
    function updateBulkMoveSelect() {
        const select = document.getElementById('bulk-move-folder');
        if (!select) return;
        
        select.innerHTML = `
            <option value="">${_('folders.move_to')}...</option>
            <option value="0">${_('folders.unassigned')}</option>
            ${folders.map(f => `<option value="${f.id}">${Utils.escapeHtml(f.name)}</option>`).join('')}
        `;
        
        // Add change listener
        select.addEventListener('change', async function() {
            if (!this.value || selectedIds.size === 0) return;
            
            try {
                await Utils.api(`/api/folders/${this.value}/devices`, {
                    method: 'POST',
                    body: { deviceIds: Array.from(selectedIds) }
                });
                Notifications.success(_('folders.devices_moved'));
                this.value = '';
                selectedIds.clear();
                updateSelectionUI();
                loadDevices();
                loadFolders();
            } catch (error) {
                Notifications.error(error.message || _('errors.server_error'));
            }
        });
    }
    
    /**
     * Select folder
     */
    function selectFolder(folderId) {
        currentFolder = folderId;
        currentPage = 1;
        
        // Update active state
        document.querySelectorAll('.folder-chip').forEach(el => {
            el.classList.toggle('active', el.dataset.folder == folderId);
        });
        
        applyFilters();
    }
    
    /**
     * Initialize folder event listeners
     */
    function initFolders() {
        // Add folder button
        document.getElementById('add-folder-btn')?.addEventListener('click', showAddFolderModal);
        
        // Special folder clicks
        document.querySelectorAll('.folder-chip[data-folder="all"], .folder-chip[data-folder="unassigned"]').forEach(el => {
            el.addEventListener('click', () => selectFolder(el.dataset.folder));
        });
    }
    
    /**
     * Initialize column visibility toggle
     */
    function initColumnVisibility() {
        const btn = document.getElementById('columns-btn');
        const menu = document.getElementById('columns-menu');
        if (!btn || !menu) return;

        // Restore saved preferences
        const saved = localStorage.getItem('devices-visible-columns');
        if (saved) {
            try {
                const hidden = JSON.parse(saved);
                menu.querySelectorAll('input[data-column]').forEach(cb => {
                    cb.checked = !hidden.includes(cb.dataset.column);
                });
            } catch (e) { /* ignore parse errors */ }
        }

        // Toggle dropdown on button click
        btn.addEventListener('click', (e) => {
            e.preventDefault();
            e.stopPropagation();
            menu.classList.toggle('show');
        });

        // Close on outside click — use contains() to handle child elements
        document.addEventListener('click', (e) => {
            if (!btn.contains(e.target) && !menu.contains(e.target)) {
                menu.classList.remove('show');
            }
        });

        // Checkbox change — stop propagation so click doesn't bubble to document
        menu.querySelectorAll('input[data-column]').forEach(cb => {
            cb.addEventListener('change', (e) => {
                e.stopPropagation();
                saveColumnPreferences();
                applyColumnVisibility();
            });
        });

        // Prevent menu clicks from closing dropdown
        menu.addEventListener('click', (e) => {
            e.stopPropagation();
        });

        // Apply initial state
        applyColumnVisibility();
    }

    /**
     * Save column visibility preferences to localStorage
     */
    function saveColumnPreferences() {
        const menu = document.getElementById('columns-menu');
        if (!menu) return;
        const hidden = [];
        menu.querySelectorAll('input[data-column]').forEach(cb => {
            if (!cb.checked) hidden.push(cb.dataset.column);
        });
        localStorage.setItem('devices-visible-columns', JSON.stringify(hidden));
    }

    /**
     * Apply column visibility to table headers and cells
     */
    function applyColumnVisibility() {
        const menu = document.getElementById('columns-menu');
        if (!menu) return;

        const hiddenColumns = [];
        menu.querySelectorAll('input[data-column]').forEach(cb => {
            if (!cb.checked) hiddenColumns.push(cb.dataset.column);
        });

        // Apply to <th> elements
        document.querySelectorAll('.devices-table th[data-column]').forEach(th => {
            th.classList.toggle('column-hidden', hiddenColumns.includes(th.dataset.column));
        });

        // Apply to <td> elements
        document.querySelectorAll('.devices-table td[data-column]').forEach(td => {
            td.classList.toggle('column-hidden', hiddenColumns.includes(td.dataset.column));
        });
    }
    
    /**
     * Show add folder modal
     */
    function showAddFolderModal() {
        const template = document.getElementById('folder-form-template');
        if (!template) return;
        
        const content = template.content.cloneNode(true);
        const formHtml = content.querySelector('form').outerHTML;
        
        Modal.show({
            title: _('folders.create'),
            content: formHtml,
            size: 'small',
            buttons: [
                { label: _('actions.cancel'), class: 'btn-secondary', onClick: () => Modal.close() },
                { label: _('actions.save'), class: 'btn-primary', onClick: () => submitFolderForm() }
            ],
            onOpen: () => {
                initColorPicker();
                document.getElementById('folder-name')?.focus();
            }
        });
    }
    
    /**
     * Edit folder
     */
    async function editFolder(folderId) {
        const folder = folders.find(f => f.id === parseInt(folderId, 10));
        if (!folder) return;
        
        const template = document.getElementById('folder-form-template');
        if (!template) return;
        
        const content = template.content.cloneNode(true);
        const formHtml = content.querySelector('form').outerHTML;
        
        Modal.show({
            title: _('folders.edit'),
            content: formHtml,
            size: 'small',
            buttons: [
                { label: _('actions.cancel'), class: 'btn-secondary', onClick: () => Modal.close() },
                { label: _('actions.save'), class: 'btn-primary', onClick: () => submitFolderForm(folderId) }
            ],
            onOpen: () => {
                initColorPicker();
                document.getElementById('folder-name').value = folder.name;
                document.getElementById('folder-color').value = folder.color;
                
                // Set active color
                document.querySelectorAll('.color-option').forEach(btn => {
                    btn.classList.toggle('active', btn.dataset.color === folder.color);
                });
            }
        });
    }
    
    /**
     * Initialize color picker
     */
    function initColorPicker() {
        document.querySelectorAll('.color-option').forEach(btn => {
            btn.addEventListener('click', () => {
                document.querySelectorAll('.color-option').forEach(b => b.classList.remove('active'));
                btn.classList.add('active');
                document.getElementById('folder-color').value = btn.dataset.color;
            });
        });
    }
    
    /**
     * Submit folder form
     */
    async function submitFolderForm(folderId = null) {
        const name = document.getElementById('folder-name')?.value.trim();
        const color = document.getElementById('folder-color')?.value;
        
        if (!name) {
            Notifications.error(_('folders.name_required'));
            return;
        }
        
        try {
            if (folderId) {
                await Utils.api(`/api/folders/${folderId}`, {
                    method: 'PATCH',
                    body: { name, color }
                });
                Notifications.success(_('folders.updated'));
            } else {
                await Utils.api('/api/folders', {
                    method: 'POST',
                    body: { name, color }
                });
                Notifications.success(_('folders.created'));
            }
            
            Modal.close();
            loadFolders();
        } catch (error) {
            Notifications.error(error.message || _('errors.server_error'));
        }
    }
    
    /**
     * Delete folder
     */
    async function deleteFolder(folderId) {
        const folder = folders.find(f => f.id === parseInt(folderId, 10));
        if (!folder) return;
        
        const confirmed = await Modal.confirm({
            title: _('folders.delete'),
            message: _('folders.delete_confirm'),
            confirmLabel: _('actions.delete'),
            danger: true
        });
        
        if (!confirmed) return;
        
        try {
            await Utils.api(`/api/folders/${folderId}`, { method: 'DELETE' });
            Notifications.success(_('folders.delete_success'));
            
            // If current folder was deleted, switch to all
            if (currentFolder == folderId) {
                selectFolder('all');
            }
            
            loadFolders();
            loadDevices();
        } catch (error) {
            Notifications.error(error.message || _('errors.server_error'));
        }
    }
    
    // ==================== Drag & Drop ====================
    
    /**
     * Initialize drag & drop - row drag events only (called once)
     */
    function initDragDrop() {
        // Handle drag start on rows
        tableBody?.addEventListener('dragstart', (e) => {
            const row = e.target.closest('tr');
            if (!row) return;
            
            draggedDeviceId = row.dataset.id;
            row.classList.add('dragging');
            e.dataTransfer.effectAllowed = 'move';
            e.dataTransfer.setData('text/plain', draggedDeviceId);
        });
        
        tableBody?.addEventListener('dragend', (e) => {
            const row = e.target.closest('tr');
            if (row) row.classList.remove('dragging');
            draggedDeviceId = null;
            
            // Remove drop indicators
            document.querySelectorAll('.folder-chip.drag-over').forEach(el => {
                el.classList.remove('drag-over');
            });
        });
    }
    
    /**
     * Attach drag events to folder items (called after renderFolders)
     */
    function attachFolderDropEvents() {
        // Handle drop on ALL folders (static + dynamic)
        document.querySelectorAll('.folder-chip').forEach(folder => {
            // Skip if already has drag handlers (check with data attribute)
            if (folder.dataset.dragAttached) return;
            folder.dataset.dragAttached = 'true';
            
            folder.addEventListener('dragover', (e) => {
                e.preventDefault();
                e.dataTransfer.dropEffect = 'move';
                folder.classList.add('drag-over');
            });
            
            folder.addEventListener('dragleave', () => {
                folder.classList.remove('drag-over');
            });
            
            folder.addEventListener('drop', async (e) => {
                e.preventDefault();
                folder.classList.remove('drag-over');
                
                const deviceId = e.dataTransfer.getData('text/plain');
                if (!deviceId) return;
                
                const targetFolder = folder.dataset.folder;
                let folderId = null;
                
                if (targetFolder === 'all') {
                    return; // Can't drop on "all"
                } else if (targetFolder === 'unassigned') {
                    folderId = null;
                } else {
                    folderId = parseInt(targetFolder, 10);
                }
                
                try {
                    await Utils.api(`/api/devices/${deviceId}/folder`, {
                        method: 'PATCH',
                        body: { folderId }
                    });
                    
                    Notifications.success(_('folders.device_assigned'));
                    loadDevices();
                    loadFolders();
                } catch (error) {
                    console.error('Folder assignment failed:', error);
                    Notifications.error(error.message || _('errors.server_error'));
                }
            });
        });
    }
    
})();

/**
 * DeviceList — full device table with search, filters, device actions dropdown
 */
import { createSignal, createMemo, onMount, Show, For } from 'solid-js';
import { t } from '../lib/i18n';
import { getDevices, banDevice, sendDeviceAction, wakeOnLan, type Device } from '../lib/api';
import { toastSuccess, toastError } from '../stores/toast';

interface DeviceListProps {
    onNavigate: (panel: string) => void;
    onDeviceDetail?: (id: string) => void;
}

type Filter = 'all' | 'online' | 'offline';

export default function DeviceList(props: DeviceListProps) {
    const [devices, setDevices] = createSignal<Device[]>([]);
    const [loading, setLoading] = createSignal(true);
    const [search, setSearch] = createSignal('');
    const [filter, setFilter] = createSignal<Filter>('all');
    const [actionMenu, setActionMenu] = createSignal<string | null>(null);

    onMount(async () => {
        await loadDevices();
    });

    async function loadDevices() {
        setLoading(true);
        try {
            const list = await getDevices();
            setDevices(list);
        } catch {
            // silently handle
        } finally {
            setLoading(false);
        }
    }

    const filtered = createMemo(() => {
        let list = devices();
        const q = search().toLowerCase().trim();

        if (q) {
            list = list.filter(d =>
                d.id.toLowerCase().includes(q) ||
                (d.hostname || '').toLowerCase().includes(q) ||
                (d.platform || '').toLowerCase().includes(q) ||
                (d.tags || '').toLowerCase().includes(q)
            );
        }

        if (filter() === 'online') {
            list = list.filter(d => d.online || d.status === 'online');
        } else if (filter() === 'offline') {
            list = list.filter(d => !d.online && d.status !== 'online');
        }

        return list;
    });

    function formatLastSeen(iso: string): string {
        if (!iso) return '—';
        try {
            const d = new Date(iso);
            const now = Date.now();
            const diff = now - d.getTime();
            if (diff < 60_000) return 'just now';
            if (diff < 3600_000) return `${Math.floor(diff / 60_000)}m ago`;
            if (diff < 86400_000) return `${Math.floor(diff / 3600_000)}h ago`;
            return d.toLocaleDateString();
        } catch {
            return iso;
        }
    }

    function isOnline(device: Device): boolean {
        return device.online || device.status === 'online';
    }

    function toggleActions(e: MouseEvent, id: string) {
        e.stopPropagation();
        setActionMenu(actionMenu() === id ? null : id);
    }

    // Close action menu when clicking outside
    function handleBodyClick() {
        setActionMenu(null);
    }

    async function deviceAction(deviceId: string, action: string) {
        setActionMenu(null);
        try {
            if (action === 'wol') {
                await wakeOnLan(deviceId);
            } else if (action === 'ban') {
                await banDevice(deviceId);
            } else {
                await sendDeviceAction(deviceId, action);
            }
            toastSuccess(t('devices.action_sent'), action);
            if (action === 'ban') await loadDevices();
        } catch {
            toastError(t('devices.action_failed'));
        }
    }

    return (
        <div class="page-enter" onClick={handleBodyClick}>
            <div class="device-table-container">
                {/* Toolbar */}
                <div class="device-toolbar">
                    <input
                        type="text"
                        class="form-input device-search"
                        placeholder={t('devices.search_placeholder')}
                        value={search()}
                        onInput={(e) => setSearch(e.currentTarget.value)}
                    />
                    <div class="filter-pills">
                        <button
                            class={`filter-pill ${filter() === 'all' ? 'active' : ''}`}
                            onClick={() => setFilter('all')}
                        >
                            {t('devices.filter_all')} ({devices().length})
                        </button>
                        <button
                            class={`filter-pill ${filter() === 'online' ? 'active' : ''}`}
                            onClick={() => setFilter('online')}
                        >
                            {t('devices.filter_online')} ({devices().filter(d => isOnline(d)).length})
                        </button>
                        <button
                            class={`filter-pill ${filter() === 'offline' ? 'active' : ''}`}
                            onClick={() => setFilter('offline')}
                        >
                            {t('devices.filter_offline')} ({devices().filter(d => !isOnline(d)).length})
                        </button>
                    </div>
                    <button class="btn-icon" title={t('common.retry')} onClick={loadDevices} style="margin-left: auto;">
                        <span class="material-symbols-rounded">refresh</span>
                    </button>
                </div>

                {/* Table */}
                <Show when={!loading()} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                    <Show when={filtered().length > 0} fallback={
                        <div class="empty-state">
                            <span class="material-symbols-rounded">search_off</span>
                            <div class="empty-state-text">{t('devices.no_devices')}</div>
                        </div>
                    }>
                        <table class="device-table">
                            <thead>
                                <tr>
                                    <th>{t('devices.col_id')}</th>
                                    <th>{t('devices.col_hostname')}</th>
                                    <th>{t('devices.col_platform')}</th>
                                    <th>{t('devices.col_status')}</th>
                                    <th>{t('devices.col_last_seen')}</th>
                                    <th style="width: 80px;"></th>
                                </tr>
                            </thead>
                            <tbody>
                                <For each={filtered()}>
                                    {(device) => (
                                        <tr onClick={() => props.onDeviceDetail ? props.onDeviceDetail(device.id) : props.onNavigate(`remote:${device.id}`)}>
                                            <td style="font-family: var(--font-mono); font-size: var(--font-size-sm);">
                                                {device.id}
                                            </td>
                                            <td>{device.hostname || '—'}</td>
                                            <td>{device.platform || '—'}</td>
                                            <td>
                                                <span class={`status-dot ${isOnline(device) ? 'online' : 'offline'}`} />
                                                {isOnline(device) ? t('devices.status_online') : t('devices.status_offline')}
                                            </td>
                                            <td style="color: var(--text-secondary);">
                                                {formatLastSeen(device.last_online)}
                                            </td>
                                            <td onClick={(e) => e.stopPropagation()}>
                                                <div class="device-actions-cell">
                                                    <button class="btn-icon" title={isOnline(device) ? t('dashboard.connect') : t('remote.device_offline')} disabled={!isOnline(device)} onClick={() => props.onNavigate(`remote:${device.id}`)} style={!isOnline(device) ? 'opacity: 0.35; cursor: not-allowed;' : ''}>
                                                        <span class="material-symbols-rounded" style="font-size: 18px;">desktop_windows</span>
                                                    </button>
                                                    <div class="dropdown-wrapper">
                                                        <button class="btn-icon" title={t('devices.actions')} onClick={(e) => toggleActions(e, device.id)}>
                                                            <span class="material-symbols-rounded" style="font-size: 18px;">more_vert</span>
                                                        </button>
                                                        <Show when={actionMenu() === device.id}>
                                                            <div class="dropdown-menu">
                                                                <button class="dropdown-item" onClick={() => deviceAction(device.id, 'lock')}>
                                                                    <span class="material-symbols-rounded">lock</span>{t('devices.action_lock')}
                                                                </button>
                                                                <button class="dropdown-item" onClick={() => deviceAction(device.id, 'restart')}>
                                                                    <span class="material-symbols-rounded">restart_alt</span>{t('devices.action_restart')}
                                                                </button>
                                                                <button class="dropdown-item" onClick={() => deviceAction(device.id, 'shutdown')}>
                                                                    <span class="material-symbols-rounded">power_settings_new</span>{t('devices.action_shutdown')}
                                                                </button>
                                                                <Show when={!isOnline(device)}>
                                                                    <button class="dropdown-item" onClick={() => deviceAction(device.id, 'wol')}>
                                                                        <span class="material-symbols-rounded">power</span>{t('devices.action_wol')}
                                                                    </button>
                                                                </Show>
                                                                <div class="dropdown-divider" />
                                                                <button class="dropdown-item danger" onClick={() => deviceAction(device.id, 'ban')}>
                                                                    <span class="material-symbols-rounded">block</span>{t('devices.action_ban')}
                                                                </button>
                                                            </div>
                                                        </Show>
                                                    </div>
                                                </div>
                                            </td>
                                        </tr>
                                    )}
                                </For>
                            </tbody>
                        </table>
                    </Show>
                </Show>
            </div>
        </div>
    );
}

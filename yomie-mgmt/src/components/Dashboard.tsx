/**
 * Dashboard — overview with stat cards, quick connect, recent sessions, help requests
 */
import { createSignal, onMount, Show, For } from 'solid-js';
import { t } from '../lib/i18n';
import { getDevices, getServerHealth, getRecentSessions, getHelpRequests, type Device, type ServerHealth, type AuditEvent } from '../lib/api';
import { toastError } from '../stores/toast';

interface DashboardProps {
    onNavigate: (panel: string) => void;
}

export default function Dashboard(props: DashboardProps) {
    const [health, setHealth] = createSignal<ServerHealth | null>(null);
    const [devices, setDevices] = createSignal<Device[]>([]);
    const [sessions, setSessions] = createSignal<AuditEvent[]>([]);
    const [helpRequests, setHelpRequests] = createSignal<AuditEvent[]>([]);
    const [loading, setLoading] = createSignal(true);
    const [connectId, setConnectId] = createSignal('');

    onMount(async () => {
        await loadData();
    });

    async function loadData() {
        setLoading(true);
        try {
            const [h, d, s, hr] = await Promise.all([
                getServerHealth().catch(() => null),
                getDevices().catch(() => []),
                getRecentSessions(5).catch(() => []),
                getHelpRequests().catch(() => []),
            ]);
            setHealth(h);
            setDevices(d);
            setSessions(s);
            setHelpRequests(hr);
        } catch {
            toastError(t('common.error'));
        } finally {
            setLoading(false);
        }
    }

    function onlineCount() {
        return devices().filter(d => d.online || d.status === 'online').length;
    }

    function handleQuickConnect() {
        const id = connectId().trim();
        if (id) {
            // Navigate to remote view with device ID
            props.onNavigate(`remote:${id}`);
        }
    }

    return (
        <div class="page-enter">
            {/* Stat Cards */}
            <div class="dashboard-grid">
                <div class="stat-card">
                    <div class="stat-icon blue">
                        <span class="material-symbols-rounded">dns</span>
                    </div>
                    <div class="stat-info">
                        <div class="stat-value">
                            <Show when={health()} fallback="—">
                                <span class={`status-indicator ${health()!.status === 'ok' ? 'connected' : 'disconnected'}`}>
                                    <span class="status-dot online" />
                                    {t('dashboard.connected')}
                                </span>
                            </Show>
                        </div>
                        <div class="stat-label">{t('dashboard.server_status')}</div>
                    </div>
                </div>

                <div class="stat-card">
                    <div class="stat-icon green">
                        <span class="material-symbols-rounded">wifi</span>
                    </div>
                    <div class="stat-info">
                        <div class="stat-value">{onlineCount()}</div>
                        <div class="stat-label">{t('dashboard.online_devices')}</div>
                    </div>
                </div>

                <div class="stat-card">
                    <div class="stat-icon orange">
                        <span class="material-symbols-rounded">devices</span>
                    </div>
                    <div class="stat-info">
                        <div class="stat-value">{devices().length}</div>
                        <div class="stat-label">{t('dashboard.total_devices')}</div>
                    </div>
                </div>
            </div>

            {/* Quick Connect */}
            <div class="section-title">{t('dashboard.quick_connect')}</div>
            <div class="quick-connect">
                <input
                    type="text"
                    class="form-input"
                    placeholder={t('dashboard.quick_connect_placeholder')}
                    value={connectId()}
                    onInput={(e) => setConnectId(e.currentTarget.value)}
                    onKeyDown={(e) => e.key === 'Enter' && handleQuickConnect()}
                />
                <button class="btn-primary" onClick={handleQuickConnect} style="width: auto; padding: 8px 20px;">
                    {t('dashboard.connect')}
                </button>
            </div>

            {/* Recent Online Devices */}
            <div class="section-title">{t('dashboard.online_devices')}</div>
            <Show when={!loading()} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                <Show when={devices().filter(d => d.online || d.status === 'online').length > 0} fallback={
                    <div class="empty-state">
                        <span class="material-symbols-rounded">devices_off</span>
                        <div class="empty-state-text">{t('devices.no_devices')}</div>
                    </div>
                }>
                    <div class="device-table-container">
                        <table class="device-table">
                            <thead>
                                <tr>
                                    <th>{t('devices.col_id')}</th>
                                    <th>{t('devices.col_hostname')}</th>
                                    <th>{t('devices.col_platform')}</th>
                                    <th>{t('devices.col_status')}</th>
                                </tr>
                            </thead>
                            <tbody>
                                {devices()
                                    .filter(d => d.online || d.status === 'online')
                                    .slice(0, 10)
                                    .map(device => (
                                        <tr onClick={() => props.onNavigate(`remote:${device.id}`)}>
                                            <td>{device.id}</td>
                                            <td>{device.hostname || '—'}</td>
                                            <td>{device.platform || '—'}</td>
                                            <td>
                                                <span class="status-dot online" />
                                                {t('devices.status_online')}
                                            </td>
                                        </tr>
                                    ))}
                            </tbody>
                        </table>
                    </div>
                </Show>
            </Show>

            {/* Recent Sessions */}
            <div class="section-title">{t('dashboard.recent_sessions')}</div>
            <Show when={!loading()}>
                <Show when={sessions().length > 0} fallback={
                    <div class="empty-state small">
                        <span class="material-symbols-rounded">history</span>
                        <div class="empty-state-text">{t('dashboard.no_sessions')}</div>
                    </div>
                }>
                    <div class="device-table-container">
                        <table class="device-table">
                            <thead>
                                <tr>
                                    <th>{t('dashboard.session_device')}</th>
                                    <th>{t('dashboard.session_operator')}</th>
                                    <th>{t('dashboard.session_time')}</th>
                                </tr>
                            </thead>
                            <tbody>
                                <For each={sessions()}>
                                    {(s) => (
                                        <tr>
                                            <td>{s.target || '—'}</td>
                                            <td>{s.actor || '—'}</td>
                                            <td>{new Date(s.created_at).toLocaleString()}</td>
                                        </tr>
                                    )}
                                </For>
                            </tbody>
                        </table>
                    </div>
                </Show>
            </Show>

            {/* Help Requests */}
            <div class="section-title">{t('dashboard.help_requests')}</div>
            <Show when={!loading()}>
                <Show when={helpRequests().length > 0} fallback={
                    <div class="empty-state small">
                        <span class="material-symbols-rounded">support_agent</span>
                        <div class="empty-state-text">{t('dashboard.no_help_requests')}</div>
                    </div>
                }>
                    <div class="device-table-container">
                        <table class="device-table">
                            <thead>
                                <tr>
                                    <th>{t('dashboard.help_device')}</th>
                                    <th>{t('dashboard.help_details')}</th>
                                    <th>{t('dashboard.help_time')}</th>
                                    <th></th>
                                </tr>
                            </thead>
                            <tbody>
                                <For each={helpRequests()}>
                                    {(hr) => (
                                        <tr>
                                            <td>{hr.target || hr.actor || '—'}</td>
                                            <td>{hr.details || '—'}</td>
                                            <td>{new Date(hr.created_at).toLocaleString()}</td>
                                            <td>
                                                <button class="btn-icon" title={t('dashboard.connect')} onClick={() => props.onNavigate(`remote:${hr.target || hr.actor}`)}>
                                                    <span class="material-symbols-rounded">open_in_new</span>
                                                </button>
                                            </td>
                                        </tr>
                                    )}
                                </For>
                            </tbody>
                        </table>
                    </div>
                </Show>
            </Show>
        </div>
    );
}

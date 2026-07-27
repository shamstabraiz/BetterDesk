/**
 * ServerPanel — server management with tabs: Health, Clients, Operators, Audit
 *
 * Uses api.ts session-cookie requests (via api_proxy IPC) for all data.
 * No Bearer tokens needed — the session cookie from login is sufficient.
 */
import { createSignal, createResource, Show, For, Switch, Match } from 'solid-js';
import { t } from '../lib/i18n';
import { toastSuccess, toastError } from '../stores/toast';
import {
    getServerHealth, getServerStatus, getServerBandwidth,
    getDevices, getUsers, getAuditLog,
    banDevice,
    type Device, type PanelUser, type AuditLogEntry,
} from '../lib/api';

type Tab = 'health' | 'clients' | 'operators' | 'audit';

export default function ServerPanel() {
    const [tab, setTab] = createSignal<Tab>('health');

    // Health tab: combined stats + server status + bandwidth
    const [health, { refetch: refetchHealth }] = createResource(
        () => tab() === 'health',
        async (active) => {
            if (!active) return null;
            const [stats, status, bandwidth] = await Promise.all([
                getServerHealth(),
                getServerStatus(),
                getServerBandwidth(),
            ]);
            return { stats, status, bandwidth };
        }
    );

    // Clients tab: online devices
    const [clients, { refetch: refetchClients }] = createResource(
        () => tab() === 'clients',
        async (active) => {
            if (!active) return [];
            const devices = await getDevices();
            return devices.filter(d => d.online || d.status === 'online');
        }
    );

    // Operators tab: panel users
    const [operators] = createResource(
        () => tab() === 'operators',
        async (active) => active ? getUsers() : []
    );

    // Audit tab: recent audit log
    const [audit, { refetch: refetchAudit }] = createResource(
        () => tab() === 'audit',
        async (active) => active ? getAuditLog(100) : []
    );

    async function doBan(id: string) {
        if (!confirm(t('server.confirm_ban'))) return;
        try {
            await banDevice(id);
            toastSuccess(t('server.client_banned'));
            refetchClients();
        } catch {
            toastError(t('server.action_failed'));
        }
    }

    function formatUptime(secs?: number): string {
        if (!secs) return '—';
        const d = Math.floor(secs / 86400);
        const h = Math.floor((secs % 86400) / 3600);
        const m = Math.floor((secs % 3600) / 60);
        if (d > 0) return `${d}d ${h}h`;
        if (h > 0) return `${h}h ${m}m`;
        return `${m}m`;
    }

    function formatTime(iso?: string): string {
        if (!iso) return '—';
        try { return new Date(iso).toLocaleString(); } catch { return iso; }
    }

    const tabs: { id: Tab; icon: string; labelKey: string }[] = [
        { id: 'health', icon: 'monitor_heart', labelKey: 'server.tab_health' },
        { id: 'clients', icon: 'people', labelKey: 'server.tab_clients' },
        { id: 'operators', icon: 'admin_panel_settings', labelKey: 'server.tab_operators' },
        { id: 'audit', icon: 'history', labelKey: 'server.tab_audit' },
    ];

    return (
        <div class="page-enter">
            {/* Tabs */}
            <div class="detail-tabs" style="margin-bottom: 20px;">
                <For each={tabs}>
                    {(entry) => (
                        <button
                            class={`detail-tab ${tab() === entry.id ? 'active' : ''}`}
                            onClick={() => setTab(entry.id)}
                        >
                            <span class="material-symbols-rounded" style="font-size: 16px; margin-right: 4px; vertical-align: -3px;">{entry.icon}</span>
                            {t(entry.labelKey)}
                        </button>
                    )}
                </For>
            </div>

            <Switch>
                {/* Health */}
                <Match when={tab() === 'health'}>
                    <div class="panel-card">
                        <div class="panel-card-header">
                            <span>{t('server.tab_health')}</span>
                            <button class="btn-icon" onClick={() => refetchHealth()} title={t('common.retry')}>
                                <span class="material-symbols-rounded">refresh</span>
                            </button>
                        </div>
                        <Show when={health()} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                            {(h) => {
                                const stats = h().stats;
                                const status = h().status;
                                const bw = h().bandwidth;
                                return (
                                    <div class="stat-grid-4">
                                        <div class="stat-card">
                                            <div class={`stat-icon ${stats.status === 'ok' ? 'green' : 'red'}`}>
                                                <span class="material-symbols-rounded">{stats.status === 'ok' ? 'check_circle' : 'error'}</span>
                                            </div>
                                            <div class="stat-info">
                                                <span class="stat-value">{stats.status === 'ok' ? 'Online' : stats.status}</span>
                                                <span class="stat-label">{t('server.uptime')}</span>
                                            </div>
                                        </div>
                                        <div class="stat-card">
                                            <div class="stat-icon blue"><span class="material-symbols-rounded">devices</span></div>
                                            <div class="stat-info">
                                                <span class="stat-value">{stats.online_count} / {stats.total_count}</span>
                                                <span class="stat-label">{t('server.peers_online')}</span>
                                            </div>
                                        </div>
                                        <div class="stat-card">
                                            <div class="stat-icon green"><span class="material-symbols-rounded">timer</span></div>
                                            <div class="stat-info">
                                                <span class="stat-value">{formatUptime(status.uptime)}</span>
                                                <span class="stat-label">{t('server.uptime')}</span>
                                            </div>
                                        </div>
                                        <div class="stat-card">
                                            <div class="stat-icon orange"><span class="material-symbols-rounded">swap_horiz</span></div>
                                            <div class="stat-info">
                                                <span class="stat-value">{(bw as Record<string, unknown>).relay_active ?? 0}</span>
                                                <span class="stat-label">{t('server.relay_sessions')}</span>
                                            </div>
                                        </div>
                                    </div>
                                );
                            }}
                        </Show>
                    </div>
                </Match>

                {/* Clients */}
                <Match when={tab() === 'clients'}>
                    <div class="panel-card">
                        <div class="panel-card-header">
                            <span>{t('server.tab_clients')}</span>
                            <button class="btn-icon" onClick={() => refetchClients()} title={t('common.retry')}>
                                <span class="material-symbols-rounded">refresh</span>
                            </button>
                        </div>
                        <Show when={!clients.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                            <Show when={(clients() || []).length > 0} fallback={
                                <div class="empty-state"><span class="material-symbols-rounded">group_off</span><div class="empty-state-text">{t('server.no_clients')}</div></div>
                            }>
                                <table class="device-table">
                                    <thead><tr>
                                        <th>ID</th><th>{t('server.hostname')}</th><th>{t('server.platform')}</th><th>{t('server.last_online')}</th><th style="width: 80px;"></th>
                                    </tr></thead>
                                    <tbody>
                                        <For each={clients() || []}>
                                            {(c: Device) => (
                                                <tr>
                                                    <td style="font-family: var(--font-mono);">{c.id}</td>
                                                    <td>{c.hostname || '—'}</td>
                                                    <td>{c.platform || '—'}</td>
                                                    <td>{formatTime(c.last_online)}</td>
                                                    <td>
                                                        <div style="display: flex; gap: 4px;">
                                                            <button class="btn-icon" title={t('server.ban')} onClick={() => doBan(c.id)} style="color: var(--accent-red);">
                                                                <span class="material-symbols-rounded" style="font-size: 16px;">block</span>
                                                            </button>
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
                </Match>

                {/* Operators */}
                <Match when={tab() === 'operators'}>
                    <div class="panel-card">
                        <div class="panel-card-header"><span>{t('server.tab_operators')}</span></div>
                        <Show when={!operators.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                            <Show when={(operators() || []).length > 0} fallback={
                                <div class="empty-state"><span class="material-symbols-rounded">admin_panel_settings</span><div class="empty-state-text">{t('server.no_operators')}</div></div>
                            }>
                                <table class="device-table">
                                    <thead><tr>
                                        <th>{t('server.username')}</th><th>{t('server.role')}</th><th>{t('server.last_login')}</th><th>2FA</th>
                                    </tr></thead>
                                    <tbody>
                                        <For each={operators() || []}>
                                            {(op: PanelUser) => (
                                                <tr>
                                                    <td>{op.username}</td>
                                                    <td><span class={`role-badge ${op.role}`}>{op.role}</span></td>
                                                    <td>{formatTime(op.last_login)}</td>
                                                    <td>
                                                        <span class="material-symbols-rounded" style={`font-size: 16px; color: ${op.totp_enabled ? 'var(--accent-green)' : 'var(--text-tertiary)'};`}>
                                                            {op.totp_enabled ? 'verified_user' : 'no_encryption'}
                                                        </span>
                                                    </td>
                                                </tr>
                                            )}
                                        </For>
                                    </tbody>
                                </table>
                            </Show>
                        </Show>
                    </div>
                </Match>

                {/* Audit */}
                <Match when={tab() === 'audit'}>
                    <div class="panel-card">
                        <div class="panel-card-header">
                            <span>{t('server.tab_audit')}</span>
                            <button class="btn-icon" onClick={() => refetchAudit()} title={t('common.retry')}>
                                <span class="material-symbols-rounded">refresh</span>
                            </button>
                        </div>
                        <Show when={!audit.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                            <Show when={(audit() || []).length > 0} fallback={
                                <div class="empty-state"><span class="material-symbols-rounded">history</span><div class="empty-state-text">{t('server.no_audit')}</div></div>
                            }>
                                <table class="device-table">
                                    <thead><tr>
                                        <th>{t('server.action')}</th><th>{t('server.actor')}</th><th>{t('server.details')}</th><th>IP</th><th>{t('server.time')}</th>
                                    </tr></thead>
                                    <tbody>
                                        <For each={audit() || []}>
                                            {(entry: AuditLogEntry) => (
                                                <tr>
                                                    <td><span class={`action-badge action-${actionColor(entry.action)}`}>{entry.action}</span></td>
                                                    <td>{entry.actor || '—'}</td>
                                                    <td class="audit-details">{entry.details || '—'}</td>
                                                    <td style="font-family: var(--font-mono);">{entry.ip || '—'}</td>
                                                    <td>{formatTime(entry.created_at)}</td>
                                                </tr>
                                            )}
                                        </For>
                                    </tbody>
                                </table>
                            </Show>
                        </Show>
                    </div>
                </Match>
            </Switch>
        </div>
    );
}

function actionColor(action: string): string {
    if (!action) return 'gray';
    const a = action.toLowerCase();
    if (a.includes('login') || a.includes('auth')) return 'green';
    if (a.includes('ban') || a.includes('block') || a.includes('revoke')) return 'red';
    if (a.includes('fail') || a.includes('error')) return 'orange';
    return 'blue';
}

/**
 * DataGuardPanel — manage data protection policies, view events, and stats
 *
 * Uses Tauri IPC commands:
 * - operator_dataguard_get_policies
 * - operator_dataguard_save_policy
 * - operator_dataguard_delete_policy
 * - operator_dataguard_get_events
 * - operator_dataguard_get_stats
 */
import { createSignal, createResource, Show, For, Switch, Match } from 'solid-js';
import { t } from '../lib/i18n';
import { toastSuccess, toastError } from '../stores/toast';

type Tab = 'policies' | 'events' | 'stats';

interface Policy {
    id?: number;
    name: string;
    description?: string;
    policy_type?: string;
    action?: string;
    scope?: string;
    rules?: string;
    enabled?: boolean;
    created_at?: string;
}

interface DGEvent {
    id?: number;
    policy_id?: number;
    policy_name?: string;
    device_id?: string;
    event_type?: string;
    details?: string;
    severity?: string;
    created_at?: string;
}

interface DGStats {
    total_policies?: number;
    active_policies?: number;
    events_today?: number;
    events_week?: number;
    blocked_today?: number;
    top_violations?: { policy: string; count: number }[];
}

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core');
    // DataGuard IPC commands require access_token param for operator_json_request
    const token = localStorage.getItem('bd_access_token') || '';
    return invoke<T>(cmd, { accessToken: token, ...args });
}

export default function DataGuardPanel() {
    const [tab, setTab] = createSignal<Tab>('policies');

    const [policies, { refetch: refetchPolicies }] = createResource(
        () => tab() === 'policies',
        async (active) => {
            if (!active) return [];
            try {
                const res = await invokeCmd<any>('operator_dataguard_get_policies');
                return (res?.policies || res || []) as Policy[];
            } catch { return []; }
        }
    );

    const [events, { refetch: refetchEvents }] = createResource(
        () => tab() === 'events',
        async (active) => {
            if (!active) return [];
            try {
                const res = await invokeCmd<any>('operator_dataguard_get_events');
                return (res?.events || res || []) as DGEvent[];
            } catch { return []; }
        }
    );

    const [stats, { refetch: refetchStats }] = createResource(
        () => tab() === 'stats',
        async (active) => {
            if (!active) return null;
            try {
                const res = await invokeCmd<any>('operator_dataguard_get_stats');
                return (res || {}) as DGStats;
            } catch { return null; }
        }
    );

    async function togglePolicy(policy: Policy) {
        try {
            await invokeCmd('operator_dataguard_save_policy', {
                policy: { ...policy, enabled: !policy.enabled },
            });
            toastSuccess(t('dataguard.policy_updated'));
            refetchPolicies();
        } catch { toastError(t('dataguard.action_failed')); }
    }

    async function deletePolicy(id: number) {
        if (!confirm(t('dataguard.confirm_delete'))) return;
        try {
            await invokeCmd('operator_dataguard_delete_policy', {
                policyId: id,
            });
            toastSuccess(t('dataguard.policy_deleted'));
            refetchPolicies();
        } catch { toastError(t('dataguard.action_failed')); }
    }

    function formatTime(iso?: string): string {
        if (!iso) return '—';
        try { return new Date(iso).toLocaleString(); } catch { return iso; }
    }

    function severityColor(sev?: string): string {
        if (!sev) return 'blue';
        if (sev === 'critical' || sev === 'high') return 'red';
        if (sev === 'warning' || sev === 'medium') return 'orange';
        return 'blue';
    }

    function policyTypeIcon(type?: string): string {
        if (!type) return 'shield';
        if (type === 'file') return 'description';
        if (type === 'usb') return 'usb';
        if (type === 'network') return 'lan';
        if (type === 'clipboard') return 'content_paste';
        return 'shield';
    }

    const tabs: { id: Tab; icon: string; labelKey: string }[] = [
        { id: 'policies', icon: 'policy', labelKey: 'dataguard.tab_policies' },
        { id: 'events', icon: 'event_note', labelKey: 'dataguard.tab_events' },
        { id: 'stats', icon: 'analytics', labelKey: 'dataguard.tab_stats' },
    ];

    return (
        <div class="page-enter">
            <div class="detail-tabs" style="margin-bottom: 20px;">
                <For each={tabs}>
                    {(entry) => (
                        <button class={`detail-tab ${tab() === entry.id ? 'active' : ''}`} onClick={() => setTab(entry.id)}>
                            <span class="material-symbols-rounded" style="font-size: 16px; margin-right: 4px; vertical-align: -3px;">{entry.icon}</span>
                            {t(entry.labelKey)}
                        </button>
                    )}
                </For>
            </div>

            <Switch>
                {/* Policies */}
                <Match when={tab() === 'policies'}>
                    <div class="panel-card">
                        <div class="panel-card-header">
                            <span>{t('dataguard.tab_policies')}</span>
                            <button class="btn-icon" onClick={() => refetchPolicies()} title={t('common.retry')}>
                                <span class="material-symbols-rounded">refresh</span>
                            </button>
                        </div>
                        <Show when={!policies.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                            <Show when={(policies() || []).length > 0} fallback={
                                <div class="empty-state"><span class="material-symbols-rounded">policy</span><div class="empty-state-text">{t('dataguard.no_policies')}</div></div>
                            }>
                                <table class="data-table">
                                    <thead><tr>
                                        <th>{t('dataguard.policy_name')}</th>
                                        <th>{t('dataguard.policy_type')}</th>
                                        <th>{t('dataguard.policy_action')}</th>
                                        <th>{t('dataguard.status')}</th>
                                        <th></th>
                                    </tr></thead>
                                    <tbody>
                                        <For each={policies() || []}>
                                            {(p) => (
                                                <tr>
                                                    <td>
                                                        <span class="material-symbols-rounded" style="font-size: 16px; vertical-align: -3px; margin-right: 6px; color: var(--text-tertiary);">{policyTypeIcon(p.policy_type)}</span>
                                                        {p.name}
                                                    </td>
                                                    <td><span class="action-badge action-blue">{p.policy_type || '—'}</span></td>
                                                    <td><span class={`action-badge action-${p.action === 'block' ? 'red' : 'orange'}`}>{p.action || '—'}</span></td>
                                                    <td>
                                                        <button class="toggle-switch" onClick={() => togglePolicy(p)}>
                                                            <span class={`toggle-knob ${p.enabled ? 'on' : ''}`}></span>
                                                        </button>
                                                    </td>
                                                    <td>
                                                        <Show when={p.id != null}>
                                                            <button class="btn-icon btn-icon-danger" onClick={() => deletePolicy(p.id!)} title={t('common.close')}>
                                                                <span class="material-symbols-rounded">delete</span>
                                                            </button>
                                                        </Show>
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

                {/* Events */}
                <Match when={tab() === 'events'}>
                    <div class="panel-card">
                        <div class="panel-card-header">
                            <span>{t('dataguard.tab_events')}</span>
                            <button class="btn-icon" onClick={() => refetchEvents()} title={t('common.retry')}>
                                <span class="material-symbols-rounded">refresh</span>
                            </button>
                        </div>
                        <Show when={!events.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                            <Show when={(events() || []).length > 0} fallback={
                                <div class="empty-state"><span class="material-symbols-rounded">event_note</span><div class="empty-state-text">{t('dataguard.no_events')}</div></div>
                            }>
                                <table class="data-table">
                                    <thead><tr>
                                        <th>{t('dataguard.event_type')}</th>
                                        <th>{t('dataguard.device')}</th>
                                        <th>{t('dataguard.policy_name')}</th>
                                        <th>{t('dataguard.severity')}</th>
                                        <th>{t('dataguard.time')}</th>
                                    </tr></thead>
                                    <tbody>
                                        <For each={events() || []}>
                                            {(ev) => (
                                                <tr>
                                                    <td>{ev.event_type || '—'}</td>
                                                    <td><code style="font-size: var(--font-size-xs);">{ev.device_id || '—'}</code></td>
                                                    <td>{ev.policy_name || `#${ev.policy_id || '—'}`}</td>
                                                    <td><span class={`action-badge action-${severityColor(ev.severity)}`}>{ev.severity || 'info'}</span></td>
                                                    <td style="color: var(--text-tertiary); font-size: var(--font-size-xs);">{formatTime(ev.created_at)}</td>
                                                </tr>
                                            )}
                                        </For>
                                    </tbody>
                                </table>
                            </Show>
                        </Show>
                    </div>
                </Match>

                {/* Stats */}
                <Match when={tab() === 'stats'}>
                    <Show when={!stats.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                        <div class="stat-grid-4">
                            <div class="stat-card">
                                <div class="stat-icon stat-icon-blue"><span class="material-symbols-rounded">policy</span></div>
                                <div class="stat-info"><div class="stat-value">{stats()?.total_policies ?? 0}</div><div class="stat-label">{t('dataguard.stat_total_policies')}</div></div>
                            </div>
                            <div class="stat-card">
                                <div class="stat-icon stat-icon-green"><span class="material-symbols-rounded">verified_user</span></div>
                                <div class="stat-info"><div class="stat-value">{stats()?.active_policies ?? 0}</div><div class="stat-label">{t('dataguard.stat_active_policies')}</div></div>
                            </div>
                            <div class="stat-card">
                                <div class="stat-icon stat-icon-orange"><span class="material-symbols-rounded">event_note</span></div>
                                <div class="stat-info"><div class="stat-value">{stats()?.events_today ?? 0}</div><div class="stat-label">{t('dataguard.stat_events_today')}</div></div>
                            </div>
                            <div class="stat-card">
                                <div class="stat-icon stat-icon-red"><span class="material-symbols-rounded">block</span></div>
                                <div class="stat-info"><div class="stat-value">{stats()?.blocked_today ?? 0}</div><div class="stat-label">{t('dataguard.stat_blocked_today')}</div></div>
                            </div>
                        </div>

                        <Show when={stats()?.top_violations && stats()!.top_violations!.length > 0}>
                            <div class="panel-card" style="margin-top: 20px;">
                                <div class="panel-card-header"><span>{t('dataguard.top_violations')}</span></div>
                                <table class="data-table">
                                    <thead><tr>
                                        <th>{t('dataguard.policy_name')}</th>
                                        <th>{t('dataguard.violation_count')}</th>
                                    </tr></thead>
                                    <tbody>
                                        <For each={stats()!.top_violations!}>
                                            {(v) => (
                                                <tr>
                                                    <td>{v.policy}</td>
                                                    <td><span class="action-badge action-red">{v.count}</span></td>
                                                </tr>
                                            )}
                                        </For>
                                    </tbody>
                                </table>
                            </div>
                        </Show>

                        <button class="btn-icon" onClick={() => refetchStats()} style="margin-top: 16px;" title={t('common.retry')}>
                            <span class="material-symbols-rounded">refresh</span>
                        </button>
                    </Show>
                </Match>
            </Switch>
        </div>
    );
}

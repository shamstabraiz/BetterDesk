/**
 * HelpRequestsPanel — manage incoming help requests from end users
 *
 * Uses api.ts session-cookie auth to fetch/manage help requests.
 */
import { createSignal, createResource, Show, For } from 'solid-js';
import { t } from '../lib/i18n';
import { getHelpRequestsList, acceptHelpRequest, type HelpRequest } from '../lib/api';
import { toastSuccess, toastError } from '../stores/toast';

export default function HelpRequestsPanel() {
    const [filter, setFilter] = createSignal<'all' | 'pending' | 'accepted'>('all');

    const [requests, { refetch }] = createResource(async () => {
        return await getHelpRequestsList();
    });

    const filtered = () => {
        const all = requests() || [];
        const f = filter();
        if (f === 'pending') return all.filter(r => !r.status || r.status === 'pending');
        if (f === 'accepted') return all.filter(r => r.status === 'accepted' || r.status === 'in_progress');
        return all;
    };

    const pendingCount = () => (requests() || []).filter(r => !r.status || r.status === 'pending').length;

    async function doAccept(id: string) {
        try {
            await acceptHelpRequest(id);
            toastSuccess(t('help_requests.accepted'));
            refetch();
        } catch { toastError(t('help_requests.action_failed')); }
    }

    function formatTime(iso?: string): string {
        if (!iso) return '—';
        try { return new Date(iso).toLocaleString(); } catch { return iso; }
    }

    function statusColor(s?: string): string {
        if (!s || s === 'pending') return 'orange';
        if (s === 'accepted' || s === 'in_progress') return 'green';
        if (s === 'resolved' || s === 'closed') return 'blue';
        return 'blue';
    }

    return (
        <div class="page-enter">
            {/* Filter bar */}
            <div class="detail-tabs" style="margin-bottom: 20px;">
                <button class={`detail-tab ${filter() === 'all' ? 'active' : ''}`} onClick={() => setFilter('all')}>
                    {t('help_requests.filter_all')}
                </button>
                <button class={`detail-tab ${filter() === 'pending' ? 'active' : ''}`} onClick={() => setFilter('pending')}>
                    {t('help_requests.filter_pending')}
                    <Show when={pendingCount() > 0}>
                        <span class="notif-badge" style="margin-left: 6px;">{pendingCount()}</span>
                    </Show>
                </button>
                <button class={`detail-tab ${filter() === 'accepted' ? 'active' : ''}`} onClick={() => setFilter('accepted')}>
                    {t('help_requests.filter_accepted')}
                </button>
                <div style="flex: 1;" />
                <button class="btn-icon" onClick={() => refetch()} title={t('common.retry')}>
                    <span class="material-symbols-rounded">refresh</span>
                </button>
            </div>

            <div class="panel-card">
                <Show when={!requests.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                    <Show when={filtered().length > 0} fallback={
                        <div class="empty-state">
                            <span class="material-symbols-rounded">support_agent</span>
                            <div class="empty-state-text">{t('help_requests.empty')}</div>
                        </div>
                    }>
                        <div class="help-request-list">
                            <For each={filtered()}>
                                {(req) => (
                                    <div class={`help-request-item ${(!req.status || req.status === 'pending') ? 'pending' : ''}`}>
                                        <div class="help-request-header">
                                            <div class="help-request-device">
                                                <span class="material-symbols-rounded" style="font-size: 18px; color: var(--text-tertiary); margin-right: 6px;">devices</span>
                                                <code style="font-size: var(--font-size-sm);">{req.device_id || '—'}</code>
                                                <Show when={req.device_name}>
                                                    <span style="color: var(--text-secondary); margin-left: 8px;">{req.device_name}</span>
                                                </Show>
                                            </div>
                                            <div class="help-request-meta">
                                                <span class={`action-badge action-${statusColor(req.status)}`}>{req.status || 'pending'}</span>
                                            </div>
                                        </div>

                                        <Show when={req.message}>
                                            <div class="help-request-message">{req.message}</div>
                                        </Show>

                                        <div class="help-request-footer">
                                            <div class="help-request-info">
                                                <span style="color: var(--text-tertiary); font-size: var(--font-size-xs);">
                                                    {formatTime(req.created_at)}
                                                </span>
                                                <Show when={req.accepted_by}>
                                                    <span style="color: var(--text-secondary); font-size: var(--font-size-xs);">
                                                        → {req.accepted_by}
                                                    </span>
                                                </Show>
                                            </div>
                                            <Show when={!req.status || req.status === 'pending'}>
                                                <button class="btn-primary btn-sm" onClick={() => doAccept(req.id || '')}>
                                                    <span class="material-symbols-rounded" style="font-size: 16px; vertical-align: -3px; margin-right: 4px;">check_circle</span>
                                                    {t('help_requests.accept')}
                                                </button>
                                            </Show>
                                        </div>
                                    </div>
                                )}
                            </For>
                        </div>
                    </Show>
                </Show>
            </div>
        </div>
    );
}

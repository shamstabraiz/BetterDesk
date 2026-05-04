/**
 * NotificationCenter — shows recent notifications/alerts from activity log
 *
 * Uses `get_notifications` Tauri IPC (maps activity entries to notifications).
 */
import { createSignal, createResource, Show, For } from 'solid-js';
import { t } from '../lib/i18n';
import { toastSuccess, toastError } from '../stores/toast';

interface Notification {
    id: string;
    type: string;
    title: string;
    message?: string;
    read?: boolean;
    timestamp?: number;
    device_id?: string | null;
}

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
}

export default function NotificationCenter() {
    const [filterRead, setFilterRead] = createSignal<'all' | 'unread'>('all');

    const [notifications, { refetch }] = createResource(async () => {
        try {
            const result = await invokeCmd<Notification[]>('get_notifications');
            return Array.isArray(result) ? result : [];
        } catch {
            return [];
        }
    });

    const filtered = () => {
        const list = notifications() || [];
        if (filterRead() === 'unread') return list.filter(n => !n.read);
        return list;
    };

    const unreadCount = () => (notifications() || []).filter(n => !n.read).length;

    async function markRead(id: number) {
        try {
            await invokeCmd('mark_notification_read', { notificationId: id });
            refetch();
        } catch {
            toastError(t('notifications.action_failed'));
        }
    }

    async function markAllRead() {
        try {
            await invokeCmd('mark_all_notifications_read');
            toastSuccess(t('notifications.all_read'));
            refetch();
        } catch {
            toastError(t('notifications.action_failed'));
        }
    }

    async function dismiss(id: number) {
        try {
            await invokeCmd('dismiss_notification', { notificationId: id });
            refetch();
        } catch {
            toastError(t('notifications.action_failed'));
        }
    }

    function formatTime(ts?: number): string {
        if (!ts) return '';
        try {
            const d = new Date(ts);
            const now = Date.now();
            const diff = now - d.getTime();
            if (diff < 60_000) return 'just now';
            if (diff < 3600_000) return `${Math.floor(diff / 60_000)}m ago`;
            if (diff < 86400_000) return `${Math.floor(diff / 3600_000)}h ago`;
            return d.toLocaleDateString();
        } catch { return ''; }
    }

    function typeIcon(type: string): string {
        switch (type) {
            case 'alert': case 'error': return 'error';
            case 'warning': return 'warning';
            case 'help_request': return 'support_agent';
            case 'connection': return 'cable';
            case 'chat': return 'chat';
            case 'success': return 'check_circle';
            default: return 'info';
        }
    }

    function typeColor(type: string): string {
        switch (type) {
            case 'alert': case 'error': return 'var(--accent-red)';
            case 'warning': return 'var(--accent-orange)';
            case 'help_request': return 'var(--accent-purple, #a855f7)';
            case 'connection': return 'var(--accent-green)';
            case 'chat': return 'var(--accent-blue)';
            case 'success': return 'var(--accent-green)';
            default: return 'var(--accent-blue)';
        }
    }

    return (
        <div class="page-enter">
            <div class="panel-card">
                <div class="panel-card-header">
                    <div style="display: flex; align-items: center; gap: 8px;">
                        <span>{t('notifications.title')}</span>
                        <Show when={unreadCount() > 0}>
                            <span class="notif-badge">{unreadCount()}</span>
                        </Show>
                    </div>
                    <div style="display: flex; gap: 8px;">
                        <div class="filter-pills">
                            <button class={`filter-pill ${filterRead() === 'all' ? 'active' : ''}`} onClick={() => setFilterRead('all')}>
                                {t('notifications.all')}
                            </button>
                            <button class={`filter-pill ${filterRead() === 'unread' ? 'active' : ''}`} onClick={() => setFilterRead('unread')}>
                                {t('notifications.unread')} ({unreadCount()})
                            </button>
                        </div>
                        <Show when={unreadCount() > 0}>
                            <button class="btn-secondary" style="padding: 4px 10px; font-size: var(--font-size-xs);" onClick={markAllRead}>
                                {t('notifications.mark_all_read')}
                            </button>
                        </Show>
                        <button class="btn-icon" onClick={() => refetch()} title={t('common.retry')}>
                            <span class="material-symbols-rounded">refresh</span>
                        </button>
                    </div>
                </div>

                <Show when={!notifications.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                    <Show when={filtered().length > 0} fallback={
                        <div class="empty-state">
                            <span class="material-symbols-rounded">notifications_none</span>
                            <div class="empty-state-text">{t('notifications.empty')}</div>
                        </div>
                    }>
                        <div class="notif-list">
                            <For each={filtered()}>
                                {(notif) => (
                                    <div class={`notif-item ${notif.read ? 'read' : 'unread'}`}>
                                        <span class="material-symbols-rounded notif-icon" style={`color: ${typeColor(notif.type)};`}>
                                            {typeIcon(notif.type)}
                                        </span>
                                        <div class="notif-body" onClick={() => !notif.read && markRead(notif.id)}>
                                            <div class="notif-title">{notif.title}</div>
                                            <Show when={notif.message}><div class="notif-message">{notif.message}</div></Show>
                                            <div class="notif-time">{formatTime(notif.timestamp)}</div>
                                        </div>
                                        <button class="btn-icon notif-dismiss" title={t('common.close')} onClick={() => dismiss(notif.id)}>
                                            <span class="material-symbols-rounded" style="font-size: 14px;">close</span>
                                        </button>
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

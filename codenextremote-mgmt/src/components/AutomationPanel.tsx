/**
 * AutomationPanel — manage automation rules, alerts, and commands
 *
 * Uses Tauri IPC commands (get_rules, save_rule, delete_rule, get_alerts, etc.).
 */
import { createSignal, createResource, Show, For, Switch, Match } from 'solid-js';
import { t } from '../lib/i18n';
import { toastSuccess, toastError } from '../stores/toast';

type Tab = 'rules' | 'alerts' | 'commands';

interface Rule {
    id?: number;
    name: string;
    description?: string;
    trigger?: string;
    action?: string;
    enabled?: boolean;
    created_at?: string;
}

interface Alert {
    id?: number;
    rule_id?: number;
    device_id?: string;
    message?: string;
    severity?: string;
    acknowledged?: boolean;
    created_at?: string;
}

interface Command {
    id?: number;
    name: string;
    command?: string;
    target?: string;
    status?: string;
    created_at?: string;
}

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core');
    // Automation IPC commands require access_token param for operator_json_request
    const token = localStorage.getItem('bd_access_token') || '';
    return invoke<T>(cmd, { accessToken: token, ...args });
}

export default function AutomationPanel() {
    const [tab, setTab] = createSignal<Tab>('rules');

    const [rules, { refetch: refetchRules }] = createResource(
        () => tab() === 'rules',
        async (active) => {
            if (!active) return [];
            try { return await invokeCmd<Rule[]>('operator_automation_get_rules') || []; } catch { return []; }
        }
    );

    const [alerts, { refetch: refetchAlerts }] = createResource(
        () => tab() === 'alerts',
        async (active) => {
            if (!active) return [];
            try { return await invokeCmd<Alert[]>('operator_automation_get_alerts') || []; } catch { return []; }
        }
    );

    const [commands, { refetch: refetchCommands }] = createResource(
        () => tab() === 'commands',
        async (active) => {
            if (!active) return [];
            try { return await invokeCmd<Command[]>('operator_automation_get_commands') || []; } catch { return []; }
        }
    );

    async function toggleRule(rule: Rule) {
        try {
            await invokeCmd('operator_automation_save_rule', { rule: { ...rule, enabled: !rule.enabled } });
            toastSuccess(t('automation.rule_updated'));
            refetchRules();
        } catch { toastError(t('automation.action_failed')); }
    }

    async function deleteRule(id: number) {
        if (!confirm(t('automation.confirm_delete'))) return;
        try {
            await invokeCmd('operator_automation_delete_rule', { ruleId: id });
            toastSuccess(t('automation.rule_deleted'));
            refetchRules();
        } catch { toastError(t('automation.action_failed')); }
    }

    async function ackAlert(id: number) {
        try {
            await invokeCmd('operator_automation_ack_alert', { alertId: id });
            refetchAlerts();
        } catch { toastError(t('automation.action_failed')); }
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

    const tabs: { id: Tab; icon: string; labelKey: string }[] = [
        { id: 'rules', icon: 'rule', labelKey: 'automation.tab_rules' },
        { id: 'alerts', icon: 'notifications_active', labelKey: 'automation.tab_alerts' },
        { id: 'commands', icon: 'terminal', labelKey: 'automation.tab_commands' },
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
                {/* Rules */}
                <Match when={tab() === 'rules'}>
                    <div class="panel-card">
                        <div class="panel-card-header">
                            <span>{t('automation.tab_rules')}</span>
                            <button class="btn-icon" onClick={() => refetchRules()} title={t('common.retry')}>
                                <span class="material-symbols-rounded">refresh</span>
                            </button>
                        </div>
                        <Show when={!rules.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                            <Show when={(rules() || []).length > 0} fallback={
                                <div class="empty-state"><span class="material-symbols-rounded">rule</span><div class="empty-state-text">{t('automation.no_rules')}</div></div>
                            }>
                                <table class="device-table">
                                    <thead><tr>
                                        <th>{t('automation.rule_name')}</th><th>{t('automation.trigger')}</th><th>{t('automation.action')}</th><th>{t('automation.status')}</th><th style="width: 80px;"></th>
                                    </tr></thead>
                                    <tbody>
                                        <For each={rules() || []}>
                                            {(rule) => (
                                                <tr>
                                                    <td>{rule.name}</td>
                                                    <td>{rule.trigger || '—'}</td>
                                                    <td>{rule.action || '—'}</td>
                                                    <td>
                                                        <button class={`toggle-switch ${rule.enabled ? 'active' : ''}`} onClick={() => toggleRule(rule)}>
                                                            <span class="toggle-knob" />
                                                        </button>
                                                    </td>
                                                    <td>
                                                        <button class="btn-icon" style="color: var(--accent-red);" onClick={() => deleteRule(rule.id!)} title={t('common.cancel')}>
                                                            <span class="material-symbols-rounded" style="font-size: 16px;">delete</span>
                                                        </button>
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

                {/* Alerts */}
                <Match when={tab() === 'alerts'}>
                    <div class="panel-card">
                        <div class="panel-card-header">
                            <span>{t('automation.tab_alerts')}</span>
                            <button class="btn-icon" onClick={() => refetchAlerts()} title={t('common.retry')}>
                                <span class="material-symbols-rounded">refresh</span>
                            </button>
                        </div>
                        <Show when={!alerts.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                            <Show when={(alerts() || []).length > 0} fallback={
                                <div class="empty-state"><span class="material-symbols-rounded">notifications_none</span><div class="empty-state-text">{t('automation.no_alerts')}</div></div>
                            }>
                                <div class="notif-list">
                                    <For each={alerts() || []}>
                                        {(alert) => (
                                            <div class={`notif-item ${alert.acknowledged ? 'read' : 'unread'}`}>
                                                <span class="material-symbols-rounded notif-icon" style={`color: var(--accent-${severityColor(alert.severity)});`}>
                                                    {alert.severity === 'critical' ? 'error' : 'warning'}
                                                </span>
                                                <div class="notif-body">
                                                    <div class="notif-title">{alert.message || '—'}</div>
                                                    <div class="notif-time">
                                                        {alert.device_id ? `Device: ${alert.device_id} · ` : ''}{formatTime(alert.created_at)}
                                                    </div>
                                                </div>
                                                <Show when={!alert.acknowledged}>
                                                    <button class="btn-secondary" style="padding: 2px 8px; font-size: var(--font-size-xs);" onClick={() => ackAlert(alert.id!)}>
                                                        {t('automation.acknowledge')}
                                                    </button>
                                                </Show>
                                            </div>
                                        )}
                                    </For>
                                </div>
                            </Show>
                        </Show>
                    </div>
                </Match>

                {/* Commands */}
                <Match when={tab() === 'commands'}>
                    <div class="panel-card">
                        <div class="panel-card-header">
                            <span>{t('automation.tab_commands')}</span>
                            <button class="btn-icon" onClick={() => refetchCommands()} title={t('common.retry')}>
                                <span class="material-symbols-rounded">refresh</span>
                            </button>
                        </div>
                        <Show when={!commands.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                            <Show when={(commands() || []).length > 0} fallback={
                                <div class="empty-state"><span class="material-symbols-rounded">terminal</span><div class="empty-state-text">{t('automation.no_commands')}</div></div>
                            }>
                                <table class="device-table">
                                    <thead><tr>
                                        <th>{t('automation.cmd_name')}</th><th>{t('automation.cmd_target')}</th><th>{t('automation.status')}</th><th>{t('server.time')}</th>
                                    </tr></thead>
                                    <tbody>
                                        <For each={commands() || []}>
                                            {(cmd) => (
                                                <tr>
                                                    <td>{cmd.name}</td>
                                                    <td style="font-family: var(--font-mono);">{cmd.target || '—'}</td>
                                                    <td><span class={`action-badge action-${cmd.status === 'completed' ? 'green' : cmd.status === 'failed' ? 'red' : 'blue'}`}>{cmd.status || '—'}</span></td>
                                                    <td>{formatTime(cmd.created_at)}</td>
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

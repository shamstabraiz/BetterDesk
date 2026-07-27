/**
 * DeviceDetail — modal showing device information, metrics, notes, actions
 */
import { createSignal, onMount, Show, onCleanup } from 'solid-js';
import { t } from '../lib/i18n';
import { getDevice, getDeviceMetrics, updateDeviceNote, deleteDevice, type Device, type DeviceMetric } from '../lib/api';
import { toastSuccess, toastError } from '../stores/toast';

interface DeviceDetailProps {
    deviceId: string;
    onClose: () => void;
    onConnect: (id: string) => void;
    onDeleted: () => void;
}

export default function DeviceDetail(props: DeviceDetailProps) {
    const [device, setDevice] = createSignal<Device | null>(null);
    const [metrics, setMetrics] = createSignal<DeviceMetric[]>([]);
    const [loading, setLoading] = createSignal(true);
    const [note, setNote] = createSignal('');
    const [editingNote, setEditingNote] = createSignal(false);
    const [tab, setTab] = createSignal<'info' | 'metrics'>('info');

    onMount(async () => {
        await loadData();
    });

    // Close on Escape
    function handleKeyDown(e: KeyboardEvent) {
        if (e.key === 'Escape') props.onClose();
    }
    onMount(() => document.addEventListener('keydown', handleKeyDown));
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));

    async function loadData() {
        setLoading(true);
        try {
            const [d, m] = await Promise.all([
                getDevice(props.deviceId).catch(() => null),
                getDeviceMetrics(props.deviceId, 20).catch(() => []),
            ]);
            setDevice(d);
            setNote(d?.note || '');
            setMetrics(m);
        } finally {
            setLoading(false);
        }
    }

    function isOnline(): boolean {
        const d = device();
        return d ? (d.online || d.status === 'online') : false;
    }

    async function saveNote() {
        try {
            await updateDeviceNote(props.deviceId, note());
            setEditingNote(false);
            toastSuccess(t('device_detail.note_saved'));
        } catch {
            toastError(t('common.error'), t('device_detail.note_save_error'));
        }
    }

    async function handleDelete() {
        if (!confirm(t('device_detail.confirm_delete'))) return;
        try {
            await deleteDevice(props.deviceId, true);
            toastSuccess(t('device_detail.deleted'));
            props.onDeleted();
        } catch {
            toastError(t('common.error'), t('device_detail.delete_error'));
        }
    }

    function latestMetric(): DeviceMetric | null {
        const m = metrics();
        return m.length > 0 ? m[0] : null;
    }

    function formatPercent(v: number | undefined): string {
        return v != null ? `${Math.round(v)}%` : '—';
    }

    return (
        <div class="modal-overlay" onClick={(e) => { if (e.target === e.currentTarget) props.onClose(); }}>
            <div class="modal-card device-detail-modal">
                <Show when={!loading()} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                    <Show when={device()} fallback={
                        <div class="empty-state">
                            <span class="material-symbols-rounded">error</span>
                            <div class="empty-state-text">{t('device_detail.not_found')}</div>
                        </div>
                    }>
                        {/* Header */}
                        <div class="modal-header">
                            <div class="device-detail-header">
                                <span class={`status-dot ${isOnline() ? 'online' : 'offline'}`} style="width: 10px; height: 10px;" />
                                <div>
                                    <div class="modal-title">{device()!.hostname || device()!.id}</div>
                                    <div class="device-detail-sub">{device()!.id} · {device()!.platform || '—'}</div>
                                </div>
                            </div>
                            <button class="btn-icon" onClick={props.onClose}>
                                <span class="material-symbols-rounded">close</span>
                            </button>
                        </div>

                        {/* Tabs */}
                        <div class="detail-tabs">
                            <button class={`detail-tab ${tab() === 'info' ? 'active' : ''}`} onClick={() => setTab('info')}>
                                {t('device_detail.tab_info')}
                            </button>
                            <button class={`detail-tab ${tab() === 'metrics' ? 'active' : ''}`} onClick={() => setTab('metrics')}>
                                {t('device_detail.tab_metrics')}
                            </button>
                        </div>

                        {/* Info tab */}
                        <Show when={tab() === 'info'}>
                            <div class="modal-body">
                                <div class="detail-grid">
                                    <div class="detail-row">
                                        <span class="detail-label">{t('devices.col_id')}</span>
                                        <span class="detail-value" style="font-family: var(--font-mono);">{device()!.id}</span>
                                    </div>
                                    <div class="detail-row">
                                        <span class="detail-label">{t('devices.col_hostname')}</span>
                                        <span class="detail-value">{device()!.hostname || '—'}</span>
                                    </div>
                                    <div class="detail-row">
                                        <span class="detail-label">{t('devices.col_platform')}</span>
                                        <span class="detail-value">{device()!.platform || '—'}</span>
                                    </div>
                                    <div class="detail-row">
                                        <span class="detail-label">{t('device_detail.version')}</span>
                                        <span class="detail-value">{device()!.version || '—'}</span>
                                    </div>
                                    <div class="detail-row">
                                        <span class="detail-label">{t('device_detail.type')}</span>
                                        <span class="detail-value">{device()!.device_type || '—'}</span>
                                    </div>
                                    <div class="detail-row">
                                        <span class="detail-label">{t('devices.col_last_seen')}</span>
                                        <span class="detail-value">{device()!.last_online || '—'}</span>
                                    </div>
                                    <div class="detail-row">
                                        <span class="detail-label">{t('device_detail.tags')}</span>
                                        <span class="detail-value">{device()!.tags || '—'}</span>
                                    </div>
                                </div>

                                {/* Note */}
                                <div class="section-title" style="margin-top: 16px;">{t('device_detail.note')}</div>
                                <Show when={editingNote()} fallback={
                                    <div class="detail-note" onClick={() => setEditingNote(true)}>
                                        {note() || t('device_detail.note_placeholder')}
                                    </div>
                                }>
                                    <textarea
                                        class="form-input"
                                        value={note()}
                                        onInput={(e) => setNote(e.currentTarget.value)}
                                        rows={3}
                                        style="width: 100%; resize: vertical;"
                                    />
                                    <div style="display: flex; gap: 8px; margin-top: 8px;">
                                        <button class="btn-primary" style="width: auto; padding: 6px 16px;" onClick={saveNote}>
                                            {t('settings.save')}
                                        </button>
                                        <button class="btn-ghost" onClick={() => { setEditingNote(false); setNote(device()?.note || ''); }}>
                                            {t('common.cancel')}
                                        </button>
                                    </div>
                                </Show>
                            </div>
                        </Show>

                        {/* Metrics tab */}
                        <Show when={tab() === 'metrics'}>
                            <div class="modal-body">
                                <Show when={latestMetric()} fallback={
                                    <div class="empty-state" style="padding: 24px;">
                                        <span class="material-symbols-rounded">monitoring</span>
                                        <div class="empty-state-text">{t('device_detail.no_metrics')}</div>
                                    </div>
                                }>
                                    <div class="metrics-bars">
                                        <div class="metric-bar-row">
                                            <span class="metric-label">CPU</span>
                                            <div class="metric-bar-track">
                                                <div class="metric-bar-fill blue" style={`width: ${Math.min(latestMetric()!.cpu, 100)}%`} />
                                            </div>
                                            <span class="metric-value">{formatPercent(latestMetric()!.cpu)}</span>
                                        </div>
                                        <div class="metric-bar-row">
                                            <span class="metric-label">RAM</span>
                                            <div class="metric-bar-track">
                                                <div class="metric-bar-fill green" style={`width: ${Math.min(latestMetric()!.memory, 100)}%`} />
                                            </div>
                                            <span class="metric-value">{formatPercent(latestMetric()!.memory)}</span>
                                        </div>
                                        <div class="metric-bar-row">
                                            <span class="metric-label">Disk</span>
                                            <div class="metric-bar-track">
                                                <div class="metric-bar-fill orange" style={`width: ${Math.min(latestMetric()!.disk, 100)}%`} />
                                            </div>
                                            <span class="metric-value">{formatPercent(latestMetric()!.disk)}</span>
                                        </div>
                                    </div>
                                </Show>
                            </div>
                        </Show>

                        {/* Footer actions */}
                        <div class="modal-footer">
                            <button class="btn-primary" style="width: auto; padding: 8px 20px;" onClick={() => props.onConnect(props.deviceId)} disabled={!isOnline()}>
                                <span class="material-symbols-rounded" style="font-size: 16px; margin-right: 4px;">desktop_windows</span>
                                {t('dashboard.connect')}
                            </button>
                            <div style="flex: 1;" />
                            <button class="btn-ghost" style="color: var(--accent-red);" onClick={handleDelete}>
                                <span class="material-symbols-rounded" style="font-size: 16px; margin-right: 4px;">delete</span>
                                {t('device_detail.delete')}
                            </button>
                        </div>
                    </Show>
                </Show>
            </div>
        </div>
    );
}

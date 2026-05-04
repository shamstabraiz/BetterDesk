/**
 * Layout — app shell with sidebar, topbar, and content area
 */
import { createSignal, Show, Switch, Match, lazy } from 'solid-js';
import { t } from '../lib/i18n';
import { user } from '../stores/auth';
import { canView } from '../lib/permissions';
import Sidebar from './Sidebar';
import Dashboard from './Dashboard';
import DeviceList from './DeviceList';
import DeviceDetail from './DeviceDetail';
import RemoteView from './RemoteView';
import ChatPanel from './ChatPanel';
import Settings from './Settings';
import ToastContainer from './ToastContainer';

// Lazy-loaded panels (loaded only when navigated to)
const ServerPanel = lazy(() => import('./ServerPanel'));
const SessionHistoryPanel = lazy(() => import('./SessionHistoryPanel'));
const NotificationCenter = lazy(() => import('./NotificationCenter'));
const AutomationPanel = lazy(() => import('./AutomationPanel'));
const DataGuardPanel = lazy(() => import('./DataGuardPanel'));
const HelpRequestsPanel = lazy(() => import('./HelpRequestsPanel'));
const FileTransferPanel = lazy(() => import('./FileTransferPanel'));

export default function Layout() {
    const [activePanel, setActivePanel] = createSignal('dashboard');
    const [detailDeviceId, setDetailDeviceId] = createSignal<string | null>(null);

    function handleNavigate(panel: string) {
        setActivePanel(panel);
    }

    /** Open device detail modal */
    function handleDeviceDetail(id: string) {
        setDetailDeviceId(id);
    }

    /** Connect to device remote view */
    function handleConnect(id: string) {
        setDetailDeviceId(null);
        setActivePanel(`remote:${id}`);
    }

    function remoteDeviceId(): string {
        const p = activePanel();
        if (p.startsWith('remote:')) return p.substring(7);
        return '';
    }

    function panelTitle(): string {
        const p = activePanel();
        if (p === 'dashboard') return t('dashboard.title');
        if (p === 'devices') return t('devices.title');
        if (p.startsWith('remote')) return t('remote.title');
        if (p === 'chat') return t('chat.title');
        if (p === 'settings') return t('settings.title');
        if (p === 'server') return t('server.title');
        if (p === 'sessions') return t('sessions.title');
        if (p === 'notifications') return t('notifications.title');
        if (p === 'automation') return t('automation.title');
        if (p === 'dataguard') return t('dataguard.title');
        if (p === 'help_requests') return t('help_requests.title');
        if (p === 'file_transfer') return t('file_transfer.title');
        return '';
    }

    function sidebarActive(): string {
        const p = activePanel();
        if (p.startsWith('remote')) return 'remote';
        return p;
    }

    function userInitials(): string {
        const u = user();
        if (!u) return '?';
        return u.username.charAt(0).toUpperCase();
    }

    return (
        <div class="app-layout">
            <Sidebar active={sidebarActive()} onNavigate={handleNavigate} />

            <div class="main-content">
                <div class="topbar">
                    <div class="topbar-title">{panelTitle()}</div>
                    <div class="topbar-actions">
                        <Show when={user()}>
                            <div class="topbar-user">
                                <div class="topbar-avatar">{userInitials()}</div>
                                <span>{user()!.username}</span>
                                <span class="topbar-role-badge">{user()!.role?.replace('_', ' ')}</span>
                            </div>
                        </Show>
                    </div>
                </div>

                <div class="page-content">
                    <Show when={canView(sidebarActive())} fallback={
                        <div class="empty-state">
                            <span class="material-symbols-rounded">lock</span>
                            <div class="empty-state-text">{t('common.access_denied')}</div>
                            <div style="color: var(--text-tertiary); font-size: var(--font-size-sm); margin-top: 4px;">
                                {t('common.no_permission')}
                            </div>
                        </div>
                    }>
                    <Switch fallback={<Dashboard onNavigate={handleNavigate} />}>
                        <Match when={activePanel() === 'dashboard'}>
                            <Dashboard onNavigate={handleNavigate} />
                        </Match>
                        <Match when={activePanel() === 'devices'}>
                            <DeviceList onNavigate={handleNavigate} onDeviceDetail={handleDeviceDetail} />
                        </Match>
                        <Match when={activePanel().startsWith('remote:') && remoteDeviceId()}>
                            <RemoteView
                                deviceId={remoteDeviceId()}
                                onDisconnect={() => setActivePanel('devices')}
                            />
                        </Match>
                        <Match when={activePanel() === 'remote'}>
                            <div class="empty-state">
                                <span class="material-symbols-rounded">desktop_windows</span>
                                <div class="empty-state-text">{t('remote.not_connected')}</div>
                                <div style="color: var(--text-tertiary); font-size: var(--font-size-sm); margin-top: 4px;">
                                    {t('remote.connect_hint')}
                                </div>
                            </div>
                        </Match>
                        <Match when={activePanel() === 'chat'}>
                            <ChatPanel />
                        </Match>
                        <Match when={activePanel() === 'server'}>
                            <ServerPanel />
                        </Match>
                        <Match when={activePanel() === 'sessions'}>
                            <SessionHistoryPanel />
                        </Match>
                        <Match when={activePanel() === 'notifications'}>
                            <NotificationCenter />
                        </Match>
                        <Match when={activePanel() === 'automation'}>
                            <AutomationPanel />
                        </Match>
                        <Match when={activePanel() === 'dataguard'}>
                            <DataGuardPanel />
                        </Match>
                        <Match when={activePanel() === 'help_requests'}>
                            <HelpRequestsPanel />
                        </Match>
                        <Match when={activePanel() === 'file_transfer'}>
                            <FileTransferPanel />
                        </Match>
                        <Match when={activePanel() === 'settings'}>
                            <Settings />
                        </Match>
                    </Switch>
                    </Show>
                </div>
            </div>

            {/* Device Detail Modal */}
            <Show when={detailDeviceId()}>
                <DeviceDetail
                    deviceId={detailDeviceId()!}
                    onClose={() => setDetailDeviceId(null)}
                    onConnect={handleConnect}
                    onDeleted={() => { setDetailDeviceId(null); setActivePanel('devices'); }}
                />
            </Show>

            {/* Toast Notifications */}
            <ToastContainer />
        </div>
    );
}

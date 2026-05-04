/**
 * RemoteView — H.264 relay-based remote desktop viewer
 *
 * Connection flow:
 *   connect_to_peer → poll get_connection_state → password prompt →
 *   authenticate → start_remote_session → SessionManager (RGBA frames)
 */
import { createSignal, onMount, onCleanup, Show, batch } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { t } from '../lib/i18n';
import { getDevice, type Device } from '../lib/api';
import { toastError, toastInfo } from '../stores/toast';
import { log } from '../lib/logger';

interface RemoteViewProps {
    deviceId: string;
    onDisconnect: () => void;
}

type ViewState =
    | 'connecting'      // signal → punch hole → relay
    | 'authenticating'  // waiting for password
    | 'starting'        // relay → SessionManager handoff
    | 'connected'       // streaming
    | 'disconnected'
    | 'error';

interface FramePayload { width: number; height: number; rgba_b64: string; }
interface QualityPayload { fps: number; latency_ms: number; bandwidth_kbps: number; codec: string; }

export default function RemoteView(props: RemoteViewProps) {
    const [state, setState] = createSignal<ViewState>('connecting');
    const [device, setDevice] = createSignal<Device | null>(null);
    const [errorMsg, setErrorMsg] = createSignal('');
    const [password, setPassword] = createSignal('');
    const [passwordError, setPasswordError] = createSignal('');
    const [fps, setFps] = createSignal(0);
    const [latency, setLatency] = createSignal(0);
    const [peerInfo, setPeerInfo] = createSignal<Record<string, unknown> | null>(null);
    const [isFullscreen, setIsFullscreen] = createSignal(false);

    let canvasRef: HTMLCanvasElement | undefined;
    let containerRef: HTMLDivElement | undefined;
    let passwordRef: HTMLInputElement | undefined;
    const unlisteners: UnlistenFn[] = [];
    let pollTimer: ReturnType<typeof setInterval> | undefined;
    let fpsCounter = 0;
    let fpsInterval: ReturnType<typeof setInterval> | undefined;

    // ---- Lifecycle ----

    onMount(async () => {
        let d: Device | null = null;
        try {
            d = await getDevice(props.deviceId);
            setDevice(d);
        } catch {
            // Device info is optional — proceed with connection anyway
        }

        // Don't auto-connect to offline devices — show offline state instead
        const isDeviceOnline = d && (d.online || d.status === 'online');
        if (!isDeviceOnline) {
            log.info('remote', `Device ${props.deviceId} is offline — not auto-connecting`);
            batch(() => {
                setState('error');
                setErrorMsg(t('remote.device_offline'));
            });
            return;
        }

        await startConnection();
    });

    onCleanup(() => {
        cleanupAll();
    });

    function cleanupAll() {
        for (const fn of unlisteners) fn();
        unlisteners.length = 0;
        if (pollTimer) { clearInterval(pollTimer); pollTimer = undefined; }
        if (fpsInterval) { clearInterval(fpsInterval); fpsInterval = undefined; }
        // Fire-and-forget is fine here — called from onCleanup (sync)
        invoke('stop_remote_session').catch(() => {});
        invoke('disconnect').catch(() => {});
    }

    /** Async cleanup that waits for backend to fully tear down before returning. */
    async function cleanupAsync() {
        for (const fn of unlisteners) fn();
        unlisteners.length = 0;
        if (pollTimer) { clearInterval(pollTimer); pollTimer = undefined; }
        if (fpsInterval) { clearInterval(fpsInterval); fpsInterval = undefined; }
        await invoke('stop_remote_session').catch(() => {});
        await invoke('disconnect').catch(() => {});
    }

    // ---- Phase 1: Connect to peer (signal → relay) ----

    async function startConnection() {
        batch(() => {
            setState('connecting');
            setErrorMsg('');
            setPassword('');
            setPasswordError('');
        });

        try {
            // Fully tear down any previous session before reconnecting
            await cleanupAsync();

            // Initiate RustDesk protocol connection
            const currentServerUrl = localStorage.getItem('bd_mgmt_server_url') || '';
            log.info('remote', `startConnection: peer=${props.deviceId} serverUrl=${currentServerUrl}`);

            const result = await invoke<{
                state: string;
                peer_id?: string;
                peer_info?: Record<string, unknown>;
                error?: string;
            }>('connect_to_peer', { peerId: props.deviceId, serverUrl: currentServerUrl || null });

            log.info('remote', 'connect_to_peer result', result);

            if (result.state.startsWith('error')) {
                log.error('remote', `connect_to_peer error: ${result.error || result.state}`);
                batch(() => {
                    setState('error');
                    setErrorMsg(result.error || result.state);
                });
                return;
            }

            // Poll connection state until we reach authenticating
            startPolling();
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            batch(() => {
                setState('error');
                setErrorMsg(msg || t('remote.connect_failed'));
            });
            toastError(t('remote.connect_failed'), msg);
        }
    }

    function startPolling() {
        if (pollTimer) clearInterval(pollTimer);
        log.info('remote', 'startPolling: begin polling connection state every 300ms');
        pollTimer = setInterval(async () => {
            try {
                const cs = await invoke<{
                    state: string;
                    peer_id?: string;
                    peer_info?: Record<string, unknown>;
                    latency_ms?: number;
                    error?: string;
                }>('get_connection_state');

                if (cs.peer_info) setPeerInfo(cs.peer_info);
                if (cs.latency_ms) setLatency(cs.latency_ms);

                if (cs.state === 'authenticating') {
                    log.info('remote', 'poll: state=authenticating, stopping poll');
                    clearInterval(pollTimer!);
                    pollTimer = undefined;
                    setState('authenticating');
                    setTimeout(() => passwordRef?.focus(), 50);
                } else if (cs.state === 'connected') {
                    log.info('remote', 'poll: state=connected (no password required), starting session');
                    clearInterval(pollTimer!);
                    pollTimer = undefined;
                    await startSession();
                } else if (cs.state.startsWith('error') || cs.state === 'disconnected') {
                    log.error('remote', `poll: state=${cs.state} error=${cs.error}`);
                    clearInterval(pollTimer!);
                    pollTimer = undefined;
                    batch(() => {
                        setState('error');
                        setErrorMsg(cs.error || t('remote.connect_failed'));
                    });
                }
                // else: still 'connecting' — keep polling
            } catch (pollErr) {
                log.warn('remote', 'poll: transient error', pollErr);
            }
        }, 300);
    }

    // ---- Phase 2: Authenticate ----

    async function submitPassword() {
        const pw = password().trim();
        if (!pw) return;
        setPasswordError('');
        setState('starting');
        log.info('remote', `submitPassword: authenticating (pw_len=${pw.length})`);

        try {
            const result = await invoke<{
                state: string;
                peer_info?: Record<string, unknown>;
                error?: string;
            }>('authenticate', { password: pw });

            log.info('remote', 'authenticate result', result);
            if (result.peer_info) setPeerInfo(result.peer_info);

            if (result.state === 'connected' || result.state.includes('connected')) {
                log.info('remote', 'authenticate: connected, starting session');
                await startSession();
            } else if (result.state.startsWith('error')) {
                log.error('remote', `authenticate: error — ${result.error}`);
                batch(() => {
                    setState('authenticating');
                    setPasswordError(result.error || t('remote.auth_failed'));
                    setPassword('');
                });
                setTimeout(() => passwordRef?.focus(), 50);
            } else {
                log.warn('remote', `authenticate: unexpected state=${result.state}, polling`);
                startPolling();
            }
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            log.error('remote', `submitPassword catch: ${msg}`);
            batch(() => {
                setState('authenticating');
                setPasswordError(msg || t('remote.auth_failed'));
                setPassword('');
            });
            setTimeout(() => passwordRef?.focus(), 50);
        }
    }

    function handlePasswordKeyDown(e: KeyboardEvent) {
        if (e.key === 'Enter') submitPassword();
    }

    // ---- Phase 3: Start relay session (SessionManager) ----

    async function startSession() {
        setState('starting');
        log.info('remote', `startSession: peer=${props.deviceId}`);

        try {
            // Register event listeners before starting session
            unlisteners.push(await listen<FramePayload>('remote-frame', (ev) => {
                renderRgbaFrame(ev.payload);
                fpsCounter++;
            }));

            unlisteners.push(await listen<QualityPayload>('remote-quality', (ev) => {
                setFps(Math.round(ev.payload.fps));
                setLatency(ev.payload.latency_ms);
            }));

            unlisteners.push(await listen<{ reason?: string }>('remote-closed', (ev) => {
                log.warn('remote', `remote-closed event: reason=${ev.payload.reason}`);
                batch(() => {
                    setState('disconnected');
                    setErrorMsg(ev.payload.reason || '');
                });
            }));

            // Also listen for status events from start_remote_session
            unlisteners.push(await listen<{ connected: boolean; error?: string }>('remote-viewer-status', (ev) => {
                log.info('remote', `remote-viewer-status: connected=${ev.payload.connected} error=${ev.payload.error}`);
                if (!ev.payload.connected && state() === 'connected') {
                    batch(() => {
                        setState('disconnected');
                        setErrorMsg(ev.payload.error || '');
                    });
                }
            }));

            log.info('remote', 'startSession: event listeners registered, invoking start_remote_session');

            // Bridge relay → SessionManager
            await invoke('start_remote_session', { peerId: props.deviceId });

            log.info('remote', 'startSession: start_remote_session returned OK');
            setState('connected');
            toastInfo(t('remote.connected'), peerInfo()?.hostname as string || props.deviceId);

            // FPS counter — update every second
            fpsInterval = setInterval(() => {
                setFps(fpsCounter);
                fpsCounter = 0;
            }, 1000);

            // Focus canvas for keyboard input
            setTimeout(() => canvasRef?.focus(), 100);
        } catch (err: unknown) {
            const msg = err instanceof Error ? err.message : String(err);
            log.error('remote', `startSession catch: ${msg}`);
            batch(() => {
                setState('error');
                setErrorMsg(msg || t('remote.connect_failed'));
            });
            toastError(t('remote.connect_failed'), msg);
        }
    }

    // ---- RGBA Frame Rendering ----

    function renderRgbaFrame(frame: FramePayload) {
        if (!canvasRef) return;
        const ctx = canvasRef.getContext('2d');
        if (!ctx) return;

        // Resize canvas to match remote display
        if (canvasRef.width !== frame.width || canvasRef.height !== frame.height) {
            canvasRef.width = frame.width;
            canvasRef.height = frame.height;
        }

        // Decode base64 RGBA → ImageData
        const raw = atob(frame.rgba_b64);
        const expected = frame.width * frame.height * 4;
        if (raw.length < expected) return; // Incomplete frame

        const arr = new Uint8ClampedArray(expected);
        for (let i = 0; i < expected; i++) arr[i] = raw.charCodeAt(i);

        const imgData = new ImageData(arr, frame.width, frame.height);
        ctx.putImageData(imgData, 0, 0);
    }

    // ---- Disconnect ----

    function disconnect() {
        cleanupAll();
        setState('disconnected');
    }

    async function handleDisconnect() {
        await cleanupAsync();
        setState('disconnected');
        props.onDisconnect();
    }

    // ---- Mouse / Keyboard Input ----

    function canvasCoords(e: MouseEvent): { x: number; y: number } {
        if (!canvasRef) return { x: 0, y: 0 };
        const rect = canvasRef.getBoundingClientRect();
        const scaleX = canvasRef.width / rect.width;
        const scaleY = canvasRef.height / rect.height;
        return {
            x: Math.round((e.clientX - rect.left) * scaleX),
            y: Math.round((e.clientY - rect.top) * scaleY),
        };
    }

    function mouseButton(e: MouseEvent): number {
        if (e.button === 0) return 1; // left
        if (e.button === 1) return 4; // middle
        if (e.button === 2) return 2; // right
        return 1;
    }

    function sendInput(payload: Record<string, unknown>) {
        if (state() !== 'connected') return;
        invoke('send_remote_input', { input: payload }).catch(() => {});
    }

    function handleMouseMove(e: MouseEvent) {
        const { x, y } = canvasCoords(e);
        sendInput({ type: 'mouse_move', x, y });
    }

    function handleMouseDown(e: MouseEvent) {
        e.preventDefault();
        canvasRef?.focus();
        const { x, y } = canvasCoords(e);
        sendInput({ type: 'mouse_down', x, y, button: mouseButton(e) });
    }

    function handleMouseUp(e: MouseEvent) {
        const { x, y } = canvasCoords(e);
        sendInput({ type: 'mouse_up', x, y, button: mouseButton(e) });
    }

    function handleWheel(e: WheelEvent) {
        e.preventDefault();
        const { x, y } = canvasCoords(e);
        sendInput({
            type: 'wheel', x, y,
            delta_x: Math.sign(e.deltaX) * -1,
            delta_y: Math.sign(e.deltaY) * -1,
        });
    }

    function modifierFlags(e: KeyboardEvent): string[] {
        const mods: string[] = [];
        if (e.ctrlKey) mods.push('ctrl');
        if (e.shiftKey) mods.push('shift');
        if (e.altKey) mods.push('alt');
        if (e.metaKey) mods.push('meta');
        return mods;
    }

    function handleKeyDown(e: KeyboardEvent) {
        e.preventDefault();
        sendInput({ type: 'key_down', key: e.key, modifiers: modifierFlags(e) });
    }

    function handleKeyUp(e: KeyboardEvent) {
        e.preventDefault();
        sendInput({ type: 'key_up', key: e.key, modifiers: modifierFlags(e) });
    }

    function handleContextMenu(e: MouseEvent) {
        e.preventDefault();
    }

    // ---- Toolbar Actions ----

    function toggleFullscreen() {
        if (!containerRef) return;
        if (!document.fullscreenElement) {
            containerRef.requestFullscreen().then(() => setIsFullscreen(true)).catch(() => {});
        } else {
            document.exitFullscreen().then(() => setIsFullscreen(false)).catch(() => {});
        }
    }

    function requestRefresh() {
        sendInput({ type: 'refresh_video' });
    }

    function sendCtrlAltDel() {
        invoke('send_special_key', { key: 'ctrl_alt_del' }).catch(() => {});
    }

    function stateIcon(): string {
        switch (state()) {
            case 'connecting':
            case 'starting':       return 'sync';
            case 'authenticating': return 'lock';
            case 'connected':      return 'desktop_windows';
            case 'disconnected':   return 'desktop_access_disabled';
            case 'error':          return 'error';
        }
    }

    function stateColor(): string {
        switch (state()) {
            case 'connecting':
            case 'starting':
            case 'authenticating': return 'var(--accent-orange)';
            case 'connected':      return 'var(--accent-green)';
            case 'disconnected':   return 'var(--text-tertiary)';
            case 'error':          return 'var(--accent-red)';
        }
    }

    const deviceLabel = () => peerInfo()?.hostname as string || device()?.hostname || props.deviceId;

    return (
        <div class="remote-view page-enter" ref={containerRef}>
            {/* Toolbar */}
            <div class="remote-toolbar">
                <div class="remote-toolbar-left">
                    <span class="material-symbols-rounded" style={`color: ${stateColor()}; font-size: 18px;`}>
                        {stateIcon()}
                    </span>
                    <span class="remote-device-name">{deviceLabel()}</span>
                    <Show when={state() === 'connected'}>
                        <span class="remote-stats">
                            {fps()} FPS · {latency()}ms
                        </span>
                    </Show>
                </div>
                <div class="remote-toolbar-right">
                    <Show when={state() === 'connected'}>
                        <button class="btn-icon" title={t('remote.refresh')} onClick={requestRefresh}>
                            <span class="material-symbols-rounded">refresh</span>
                        </button>
                        <button class="btn-icon" title={t('remote.ctrl_alt_del')} onClick={sendCtrlAltDel}>
                            <span class="material-symbols-rounded">keyboard</span>
                        </button>
                        <button class="btn-icon" title={t('remote.fullscreen')} onClick={toggleFullscreen}>
                            <span class="material-symbols-rounded">
                                {isFullscreen() ? 'fullscreen_exit' : 'fullscreen'}
                            </span>
                        </button>
                    </Show>
                    <button class="btn-secondary" onClick={handleDisconnect} style="padding: 4px 12px;">
                        <span class="material-symbols-rounded" style="font-size: 16px; margin-right: 4px;">power_settings_new</span>
                        {t('remote.disconnect')}
                    </button>
                </div>
            </div>

            {/* Canvas / Status / Password */}
            <div class="remote-canvas-container">
                {/* Password prompt */}
                <Show when={state() === 'authenticating'}>
                    <div class="remote-status-overlay">
                        <span class="material-symbols-rounded" style="font-size: 56px; color: var(--accent-orange);">lock</span>
                        <div class="remote-status-text" style="margin-bottom: 12px;">
                            {t('remote.enter_password')}
                        </div>
                        <div class="remote-password-form">
                            <input
                                ref={passwordRef}
                                type="password"
                                class="input"
                                style="width: 260px; text-align: center;"
                                placeholder={t('remote.password_placeholder')}
                                value={password()}
                                onInput={(e) => setPassword(e.currentTarget.value)}
                                onKeyDown={handlePasswordKeyDown}
                                autocomplete="off"
                            />
                            <Show when={passwordError()}>
                                <div style="color: var(--accent-red); font-size: var(--font-size-sm); margin-top: 6px;">
                                    {passwordError()}
                                </div>
                            </Show>
                            <button class="btn-primary" style="width: auto; padding: 8px 24px; margin-top: 10px;" onClick={submitPassword}>
                                {t('remote.connect_btn')}
                            </button>
                        </div>
                    </div>
                </Show>

                {/* Connecting / Starting spinner */}
                <Show when={state() === 'connecting' || state() === 'starting'}>
                    <div class="remote-status-overlay">
                        <span class="material-symbols-rounded spinning" style="font-size: 56px; color: var(--accent-orange);">sync</span>
                        <div class="remote-status-text">
                            {state() === 'connecting' ? t('remote.connecting') : t('remote.starting_session')}
                        </div>
                    </div>
                </Show>

                {/* Error / Disconnected */}
                <Show when={state() === 'error' || state() === 'disconnected'}>
                    <div class="remote-status-overlay">
                        <span class="material-symbols-rounded" style={`font-size: 64px; color: ${stateColor()};`}>
                            {stateIcon()}
                        </span>
                        <div class="remote-status-text">
                            {state() === 'disconnected' ? t('remote.disconnected') : (errorMsg() || t('remote.connect_failed'))}
                        </div>
                        <button class="btn-primary" style="width: auto; padding: 8px 20px; margin-top: 12px;" onClick={startConnection}>
                            {t('common.retry')}
                        </button>
                    </div>
                </Show>

                {/* Remote Canvas */}
                <Show when={state() === 'connected'}>
                    <canvas
                        ref={canvasRef}
                        class="remote-canvas"
                        tabIndex={0}
                        onMouseMove={handleMouseMove}
                        onMouseDown={handleMouseDown}
                        onMouseUp={handleMouseUp}
                        onWheel={handleWheel}
                        onKeyDown={handleKeyDown}
                        onKeyUp={handleKeyUp}
                        onContextMenu={handleContextMenu}
                    />
                </Show>
            </div>
        </div>
    );
}

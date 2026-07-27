/**
 * Auth Store — manages operator authentication state
 *
 * Auth model: dual auth.
 *   1. Session cookies (express-session) — for web panel browsing via api_proxy.
 *   2. Access token (BD operator API) — for relay connections and operator API calls.
 *
 * After session login succeeds, we also request an access token from
 * POST /api/bd/operator/login and persist it in the Rust settings (OS keyring).
 */
import { createSignal } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';
import {
    initApi, setServerUrl, getServerUrl,
    clearAuth, hasStoredAuth, login, verifyTotp, checkSession, logout,
} from '../lib/api';
import { log } from '../lib/logger';

export interface User {
    username: string;
    role: string;
}

// ---- Signals ----
const [isLoggedIn, setIsLoggedIn] = createSignal(false);
const [user, setUser] = createSignal<User | null>(null);
const [isLoading, setIsLoading] = createSignal(true);

// Temporarily hold credentials for post-TOTP token acquisition
let _pendingUsername = '';
let _pendingPassword = '';

// ---- Exports ----
export { isLoggedIn, user, isLoading };

/**
 * After session login succeeds, obtain and persist an access token for relay/API.
 *
 * Uses the Rust `operator_login` IPC command directly — this avoids the complex
 * api_proxy → request → invoke chain that was silently failing.  The Rust command
 * makes a direct HTTP request to /api/bd/operator/login and stores the token
 * in Settings automatically.
 */
async function acquireAccessToken(username: string, password: string): Promise<void> {
    // Step 1: Sync console_url in Rust Settings so operator_login targets the
    // correct server (bd_api_url() depends on settings.console_url).
    try {
        const currentUrl = getServerUrl();
        log.info('auth', `acquireAccessToken: syncing console_url=${currentUrl}`);
        if (currentUrl) {
            const config = await invoke<Record<string, unknown>>('get_config');
            log.debug('auth', 'Current Rust config', { console_url: config?.console_url, server_address: config?.server_address });
            if (config && config.console_url !== currentUrl) {
                await invoke('save_config', { config: { ...config, console_url: currentUrl } });
                log.info('auth', `console_url synced: ${config.console_url} → ${currentUrl}`);
            }
        }
    } catch (e) {
        log.warn('auth', 'Failed to sync console_url', e);
    }

    // Step 2: Call operator_login IPC directly — bypasses api_proxy chain entirely.
    // The Rust command: makes HTTP POST → parses response → stores access_token
    // in Settings (OS keyring) → returns full JSON.
    try {
        log.info('auth', `acquireAccessToken: calling operator_login IPC (user=${username})`);
        const resp = await invoke<Record<string, unknown>>('operator_login', {
            username,
            password,
        });
        log.info('auth', 'operator_login IPC returned', { keys: Object.keys(resp || {}) });

        // Also store in localStorage for fallback reads
        const token = resp?.access_token;
        if (typeof token === 'string' && token.length > 0) {
            localStorage.setItem('bd_access_token', token);
            log.info('auth', `Access token saved (len=${token.length}, prefix=${token.slice(0, 8)}…)`);
        } else {
            log.warn('auth', 'operator_login response missing access_token field', resp);
        }
    } catch (e) {
        // Non-fatal — relay connections will fall back to session cookies.
        log.error('auth', 'acquireAccessToken FAILED', e);
    }
}

/** Initialize auth — check for existing session cookie */
export async function initAuth(): Promise<void> {
    initApi();
    setIsLoading(true);
    log.info('auth', 'initAuth: checking stored session...', { hasStoredAuth: hasStoredAuth(), serverUrl: getServerUrl() });

    if (hasStoredAuth()) {
        try {
            const session = await checkSession();
            log.info('auth', 'initAuth: checkSession result', session);
            if (session.valid && session.user) {
                setUser(session.user);
                setIsLoggedIn(true);
                log.info('auth', `Session restored: ${session.user.username} (${session.user.role})`);
            }
        } catch (e) {
            log.warn('auth', 'initAuth: session check failed', e);
        }
    }

    setIsLoading(false);
}

/** Login with credentials — sets session cookie, may require 2FA */
export async function doLogin(
    server: string,
    username: string,
    password: string
): Promise<{ success: boolean; totpRequired?: boolean; error?: string }> {
    try {
        log.info('auth', `doLogin: server=${server} user=${username}`);
        setServerUrl(server);
        const result = await login(username, password);
        log.info('auth', 'doLogin: session login result', { success: result.success, totpRequired: result.totpRequired, hasUser: !!result.user, error: result.error });

        if (result.totpRequired) {
            // 2FA needed — hold credentials for post-TOTP token acquisition
            _pendingUsername = username;
            _pendingPassword = password;
            log.info('auth', 'TOTP required — credentials held for post-2FA token acquisition');
            return { success: false, totpRequired: true };
        }

        if (result.success && result.user) {
            setUser(result.user);
            setIsLoggedIn(true);
            log.info('auth', `Session login OK: ${result.user.username} (${result.user.role}). Acquiring access token...`);
            // Obtain access token for relay connections (non-blocking)
            await acquireAccessToken(username, password);
            log.info('auth', 'doLogin complete — session + token acquired');
            return { success: true };
        }

        log.warn('auth', 'doLogin: login not successful', result);
        return { success: false, error: result.error || 'Login failed' };
    } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : 'Unknown error';
        log.error('auth', `doLogin exception: ${msg}`, e);
        return { success: false, error: msg };
    }
}

/** Complete 2FA verification — session already has pending user from login */
export async function doVerifyTotp(
    code: string
): Promise<{ success: boolean; error?: string }> {
    try {
        log.info('auth', 'doVerifyTotp: verifying TOTP code...');
        const result = await verifyTotp(code);
        log.info('auth', 'doVerifyTotp result', { success: result.success, hasUser: !!result.user, error: result.error });

        if (result.success && result.user) {
            setUser(result.user);
            setIsLoggedIn(true);
            // Obtain access token using held credentials
            if (_pendingUsername && _pendingPassword) {
                log.info('auth', 'TOTP verified — acquiring access token with held credentials');
                await acquireAccessToken(_pendingUsername, _pendingPassword);
            }
            // Clear held credentials
            _pendingUsername = '';
            _pendingPassword = '';
            return { success: true };
        }

        return { success: false, error: result.error || 'Verification failed' };
    } catch (e: unknown) {
        const msg = e instanceof Error ? e.message : 'Unknown error';
        log.error('auth', `doVerifyTotp exception: ${msg}`, e);
        return { success: false, error: msg };
    }
}

/** Logout — destroy server session and clear access token */
export async function doLogout(): Promise<void> {
    log.info('auth', 'doLogout: destroying session and clearing token');
    await logout();
    clearAuth();
    // Clear persisted access token
    try {
        await invoke('set_access_token', { token: '' });
    } catch { /* ignore */ }
    localStorage.removeItem('bd_access_token');
    _pendingUsername = '';
    _pendingPassword = '';
    setUser(null);
    setIsLoggedIn(false);
    log.info('auth', 'doLogout complete');
}

/** Get stored server URL */
export function getStoredServer(): string {
    return getServerUrl();
}

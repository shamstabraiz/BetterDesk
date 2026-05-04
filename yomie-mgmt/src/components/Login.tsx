/**
 * Login Component — server address + credentials + TOTP 2FA
 */
import { createSignal, Show } from 'solid-js';
import { t } from '../lib/i18n';
import { doLogin, doVerifyTotp, getStoredServer } from '../stores/auth';
import bdIcon from '../assets/bd-icon.png';

export default function Login() {
    const [server, setServer] = createSignal(getStoredServer() || '');
    const [username, setUsername] = createSignal('');
    const [password, setPassword] = createSignal('');
    const [remember, setRemember] = createSignal(true);
    const [error, setError] = createSignal('');
    const [loading, setLoading] = createSignal(false);

    // 2FA state
    const [tfaMode, setTfaMode] = createSignal(false);
    const [totpDigits, setTotpDigits] = createSignal(['', '', '', '', '', '']);

    async function handleLogin(e: Event) {
        e.preventDefault();
        setError('');

        const s = server().trim();
        const u = username().trim();
        const p = password();

        if (!s || !u || !p) {
            setError(t('login.error_empty_fields'));
            return;
        }

        // Basic URL validation
        try {
            new URL(s);
        } catch {
            setError(t('login.error_invalid_url'));
            return;
        }

        setLoading(true);
        const result = await doLogin(s, u, p);
        setLoading(false);

        if (result.totpRequired) {
            setTfaMode(true);
            return;
        }

        if (!result.success) {
            setError(result.error || t('login.error_credentials'));
        }
    }

    function handleTotpInput(index: number, value: string) {
        if (!/^\d?$/.test(value)) return;

        const digits = [...totpDigits()];
        digits[index] = value;
        setTotpDigits(digits);

        // Auto-advance to next input
        if (value && index < 5) {
            const next = document.querySelector<HTMLInputElement>(`.totp-digit[data-index="${index + 1}"]`);
            next?.focus();
        }

        // Auto-submit on 6 digits
        if (digits.every(d => d !== '')) {
            submitTotp(digits.join(''));
        }
    }

    function handleTotpKeyDown(index: number, e: KeyboardEvent) {
        if (e.key === 'Backspace' && !totpDigits()[index] && index > 0) {
            const prev = document.querySelector<HTMLInputElement>(`.totp-digit[data-index="${index - 1}"]`);
            prev?.focus();
        }
    }

    function handleTotpPaste(e: ClipboardEvent) {
        e.preventDefault();
        const pasted = e.clipboardData?.getData('text')?.replace(/\D/g, '').slice(0, 6) || '';
        if (pasted.length === 6) {
            setTotpDigits(pasted.split(''));
            submitTotp(pasted);
        }
    }

    async function submitTotp(code: string) {
        setError('');
        setLoading(true);
        const result = await doVerifyTotp(code);
        setLoading(false);

        if (!result.success) {
            setError(result.error || t('login.error_totp'));
            setTotpDigits(['', '', '', '', '', '']);
            // Focus first digit
            const first = document.querySelector<HTMLInputElement>('.totp-digit[data-index="0"]');
            first?.focus();
        }
    }

    function backToLogin() {
        setTfaMode(false);
        setTotpDigits(['', '', '', '', '', '']);
        setError('');
    }

    return (
        <div class="login-screen">
            <div class="login-card">
                <div class="login-header">
                    <img src={bdIcon} alt="Yomie" class="login-logo" />
                    <Show when={!tfaMode()} fallback={
                        <>
                            <div class="login-title">{t('login.totp_title')}</div>
                            <div class="login-subtitle">{t('login.totp_subtitle')}</div>
                        </>
                    }>
                        <div class="login-title">{t('login.title')}</div>
                        <div class="login-subtitle">{t('login.subtitle')}</div>
                    </Show>
                </div>

                <Show when={error()}>
                    <div class="login-error">
                        <span class="material-symbols-rounded">error</span>
                        {error()}
                    </div>
                </Show>

                <Show when={!tfaMode()} fallback={
                    /* -- TOTP 2FA -- */
                    <div>
                        <div class="totp-digits" onPaste={handleTotpPaste}>
                            {totpDigits().map((digit, i) => (
                                <input
                                    type="text"
                                    inputmode="numeric"
                                    maxLength={1}
                                    class="form-input totp-digit"
                                    data-index={i}
                                    value={digit}
                                    onInput={(e) => handleTotpInput(i, e.currentTarget.value)}
                                    onKeyDown={(e) => handleTotpKeyDown(i, e)}
                                    disabled={loading()}
                                    autocomplete="off"
                                />
                            ))}
                        </div>
                        <button
                            class="btn-primary"
                            onClick={() => submitTotp(totpDigits().join(''))}
                            disabled={loading() || totpDigits().some(d => !d)}
                        >
                            {loading() ? t('login.signing_in') : t('login.totp_verify')}
                        </button>
                        <div class="login-back">
                            <button class="btn-ghost" onClick={backToLogin}>
                                {t('login.totp_back')}
                            </button>
                        </div>
                    </div>
                }>
                    {/* -- Login Form -- */}
                    <form class="login-form" onSubmit={handleLogin}>
                        <div class="form-group">
                            <label class="form-label">{t('login.server_address')}</label>
                            <input
                                type="url"
                                class="form-input"
                                placeholder={t('login.server_placeholder')}
                                value={server()}
                                onInput={(e) => setServer(e.currentTarget.value)}
                                autocomplete="url"
                            />
                        </div>
                        <div class="form-group">
                            <label class="form-label">{t('login.username')}</label>
                            <input
                                type="text"
                                class="form-input"
                                value={username()}
                                onInput={(e) => setUsername(e.currentTarget.value)}
                                autocomplete="username"
                            />
                        </div>
                        <div class="form-group">
                            <label class="form-label">{t('login.password')}</label>
                            <input
                                type="password"
                                class="form-input"
                                value={password()}
                                onInput={(e) => setPassword(e.currentTarget.value)}
                                autocomplete="current-password"
                            />
                        </div>
                        <label class="form-checkbox">
                            <input
                                type="checkbox"
                                checked={remember()}
                                onChange={(e) => setRemember(e.currentTarget.checked)}
                            />
                            {t('login.remember_me')}
                        </label>
                        <button
                            type="submit"
                            class="btn-primary"
                            disabled={loading()}
                        >
                            {loading() ? t('login.signing_in') : t('login.sign_in')}
                        </button>
                    </form>
                </Show>
            </div>
        </div>
    );
}

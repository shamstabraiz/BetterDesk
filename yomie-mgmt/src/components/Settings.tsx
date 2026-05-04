/**
 * Settings — language, theme, server connection
 */
import { createSignal, onMount } from 'solid-js';
import { t, getLocale, setLocale, SUPPORTED_LOCALES } from '../lib/i18n';
import { getServerUrl, setServerUrl } from '../lib/api';
import { toastSuccess } from '../stores/toast';

type Theme = 'dark' | 'light' | 'auto';

function getStoredTheme(): Theme {
    return (localStorage.getItem('bd_theme') as Theme) || 'dark';
}

function applyTheme(theme: Theme) {
    localStorage.setItem('bd_theme', theme);
    const root = document.documentElement;
    if (theme === 'auto') {
        const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
        root.setAttribute('data-theme', prefersDark ? 'dark' : 'light');
    } else {
        root.setAttribute('data-theme', theme);
    }
}

export default function Settings() {
    const [serverAddr, setServerAddr] = createSignal(getServerUrl());
    const [selectedLocale, setSelectedLocale] = createSignal(getLocale());
    const [theme, setThemeSignal] = createSignal<Theme>(getStoredTheme());
    const [saved, setSaved] = createSignal(false);

    onMount(() => {
        applyTheme(theme());
    });

    async function handleLocaleChange(code: string) {
        setSelectedLocale(code);
        await setLocale(code);
    }

    function handleTheme(t: Theme) {
        setThemeSignal(t);
        applyTheme(t);
    }

    function handleSave() {
        setServerUrl(serverAddr());
        setSaved(true);
        toastSuccess(t('settings.saved'));
        setTimeout(() => setSaved(false), 2000);
    }

    return (
        <div class="page-enter" style="max-width: 500px;">
            {/* Language */}
            <div class="settings-section">
                <div class="settings-section-title">{t('settings.language')}</div>
                <select
                    class="form-input"
                    value={selectedLocale()}
                    onChange={(e) => handleLocaleChange(e.currentTarget.value)}
                    style="width: 100%;"
                >
                    {SUPPORTED_LOCALES.map(loc => (
                        <option value={loc.code}>{loc.flag} {loc.name}</option>
                    ))}
                </select>
            </div>

            {/* Theme */}
            <div class="settings-section">
                <div class="settings-section-title">{t('settings.theme')}</div>
                <div class="theme-buttons">
                    <button class={`theme-btn ${theme() === 'dark' ? 'active' : ''}`} onClick={() => handleTheme('dark')}>
                        {t('settings.theme_dark')}
                    </button>
                    <button class={`theme-btn ${theme() === 'light' ? 'active' : ''}`} onClick={() => handleTheme('light')}>
                        {t('settings.theme_light')}
                    </button>
                    <button class={`theme-btn ${theme() === 'auto' ? 'active' : ''}`} onClick={() => handleTheme('auto')}>
                        {t('settings.theme_auto')}
                    </button>
                </div>
            </div>

            {/* Server */}
            <div class="settings-section">
                <div class="settings-section-title">{t('settings.server')}</div>
                <div class="form-group" style="margin-bottom: 12px;">
                    <label class="form-label">{t('settings.server_address')}</label>
                    <input
                        type="url"
                        class="form-input"
                        value={serverAddr()}
                        onInput={(e) => setServerAddr(e.currentTarget.value)}
                    />
                </div>
                <button class="btn-primary" onClick={handleSave} style="width: auto; padding: 8px 24px;">
                    {saved() ? t('settings.saved') : t('settings.save')}
                </button>
            </div>
        </div>
    );
}

/**
 * FileTransferPanel — local file browser with open-in-native capability
 *
 * Uses Tauri IPC commands:
 * - browse_local_files
 * - open_file_native
 */
import { createSignal, createResource, Show, For } from 'solid-js';
import { t } from '../lib/i18n';
import { toastError } from '../stores/toast';

interface FileEntry {
    name: string;
    path: string;
    is_dir: boolean;
    size?: number;
    modified?: string;
}

async function invokeCmd<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
}

function formatSize(bytes?: number): string {
    if (bytes == null || bytes < 0) return '—';
    if (bytes === 0) return '0 B';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
    const val = bytes / Math.pow(1024, i);
    return `${val < 10 ? val.toFixed(1) : Math.round(val)} ${units[i]}`;
}

function formatTime(iso?: string): string {
    if (!iso) return '—';
    try { return new Date(iso).toLocaleString(); } catch { return iso; }
}

function fileIcon(entry: FileEntry): string {
    if (entry.is_dir) return 'folder';
    const ext = entry.name.split('.').pop()?.toLowerCase() || '';
    if (['jpg', 'jpeg', 'png', 'gif', 'svg', 'webp', 'bmp', 'ico'].includes(ext)) return 'image';
    if (['mp4', 'avi', 'mkv', 'mov', 'webm'].includes(ext)) return 'movie';
    if (['mp3', 'wav', 'ogg', 'flac', 'aac'].includes(ext)) return 'audio_file';
    if (['zip', 'tar', 'gz', 'rar', '7z'].includes(ext)) return 'folder_zip';
    if (['exe', 'msi', 'dmg', 'deb', 'rpm'].includes(ext)) return 'terminal';
    if (['pdf'].includes(ext)) return 'picture_as_pdf';
    if (['doc', 'docx', 'odt', 'rtf', 'txt', 'md'].includes(ext)) return 'description';
    if (['xls', 'xlsx', 'csv'].includes(ext)) return 'table_chart';
    if (['js', 'ts', 'py', 'go', 'rs', 'c', 'cpp', 'java', 'sh', 'ps1', 'json', 'yml', 'yaml', 'toml'].includes(ext)) return 'code';
    return 'draft';
}

function defaultPath(): string {
    try {
        const os = navigator.userAgent;
        if (os.includes('Win')) return 'C:\\';
    } catch { /* ignore */ }
    return '/';
}

export default function FileTransferPanel() {
    const [currentPath, setCurrentPath] = createSignal(defaultPath());
    const [showHidden, setShowHidden] = createSignal(false);
    const [pathInput, setPathInput] = createSignal(defaultPath());

    const [listing, { refetch }] = createResource(
        () => ({ path: currentPath(), hidden: showHidden() }),
        async ({ path, hidden }) => {
            try {
                const res = await invokeCmd<any>('browse_local_files', { path, showHidden: hidden });
                const entries = (res?.entries || res || []) as FileEntry[];
                // Sort: dirs first, then alphabetical
                return entries.sort((a, b) => {
                    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
                    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
                });
            } catch (e) {
                toastError(String(e));
                return [];
            }
        }
    );

    function navigateTo(path: string) {
        setCurrentPath(path);
        setPathInput(path);
    }

    function goUp() {
        const p = currentPath();
        // Windows: C:\ → stay, C:\foo → C:\
        // Unix: / → stay, /foo → /
        const sep = p.includes('\\') ? '\\' : '/';
        const parts = p.split(sep).filter(Boolean);
        if (parts.length <= 1 && p.startsWith('/')) { navigateTo('/'); return; }
        if (parts.length <= 1 && /^[A-Z]:/i.test(p)) { navigateTo(parts[0] + '\\'); return; }
        parts.pop();
        const parent = p.startsWith('/') ? '/' + parts.join('/') : parts.join(sep) + sep;
        navigateTo(parent);
    }

    function handleEntry(entry: FileEntry) {
        if (entry.is_dir) {
            navigateTo(entry.path);
        } else {
            openFile(entry.path);
        }
    }

    async function openFile(path: string) {
        try {
            await invokeCmd('open_file_native', { path });
        } catch (e) {
            toastError(String(e));
        }
    }

    function handlePathSubmit(e: Event) {
        e.preventDefault();
        navigateTo(pathInput());
    }

    function breadcrumbs(): { label: string; path: string }[] {
        const p = currentPath();
        const sep = p.includes('\\') ? '\\' : '/';
        const parts = p.split(sep).filter(Boolean);
        const result: { label: string; path: string }[] = [];

        if (p.startsWith('/')) {
            result.push({ label: '/', path: '/' });
        }

        let acc = p.startsWith('/') ? '/' : '';
        for (const part of parts) {
            acc += part + sep;
            result.push({ label: part, path: acc });
        }
        return result;
    }

    return (
        <div class="page-enter">
            {/* Path bar */}
            <div class="file-toolbar">
                <button class="btn-icon" onClick={goUp} title={t('file_transfer.go_up')}>
                    <span class="material-symbols-rounded">arrow_upward</span>
                </button>
                <button class="btn-icon" onClick={() => refetch()} title={t('common.retry')}>
                    <span class="material-symbols-rounded">refresh</span>
                </button>

                <form class="file-path-form" onSubmit={handlePathSubmit}>
                    <input
                        type="text"
                        class="file-path-input"
                        value={pathInput()}
                        onInput={(e) => setPathInput(e.currentTarget.value)}
                    />
                </form>

                <label class="file-hidden-toggle" title={t('file_transfer.show_hidden')}>
                    <input type="checkbox" checked={showHidden()} onChange={(e) => setShowHidden(e.currentTarget.checked)} />
                    <span class="material-symbols-rounded" style="font-size: 18px;">visibility</span>
                </label>
            </div>

            {/* Breadcrumbs */}
            <div class="file-breadcrumbs">
                <For each={breadcrumbs()}>
                    {(crumb, i) => (
                        <>
                            <Show when={i() > 0}><span class="file-breadcrumb-sep">/</span></Show>
                            <button class="file-breadcrumb" onClick={() => navigateTo(crumb.path)}>
                                {crumb.label}
                            </button>
                        </>
                    )}
                </For>
            </div>

            {/* File list */}
            <div class="panel-card">
                <Show when={!listing.loading} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                    <Show when={(listing() || []).length > 0} fallback={
                        <div class="empty-state">
                            <span class="material-symbols-rounded">folder_off</span>
                            <div class="empty-state-text">{t('file_transfer.empty')}</div>
                        </div>
                    }>
                        <table class="data-table file-table">
                            <thead><tr>
                                <th>{t('file_transfer.name')}</th>
                                <th>{t('file_transfer.size')}</th>
                                <th>{t('file_transfer.modified')}</th>
                                <th></th>
                            </tr></thead>
                            <tbody>
                                <For each={listing() || []}>
                                    {(entry) => (
                                        <tr class="file-row" onDblClick={() => handleEntry(entry)}>
                                            <td class="file-name-cell">
                                                <span class={`material-symbols-rounded file-icon ${entry.is_dir ? 'file-icon-dir' : ''}`}>{fileIcon(entry)}</span>
                                                <button class="file-name-btn" onClick={() => handleEntry(entry)}>
                                                    {entry.name}
                                                </button>
                                            </td>
                                            <td class="file-size">{entry.is_dir ? '—' : formatSize(entry.size)}</td>
                                            <td class="file-time">{formatTime(entry.modified)}</td>
                                            <td>
                                                <Show when={!entry.is_dir}>
                                                    <button class="btn-icon" onClick={() => openFile(entry.path)} title={t('file_transfer.open')}>
                                                        <span class="material-symbols-rounded">open_in_new</span>
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
        </div>
    );
}

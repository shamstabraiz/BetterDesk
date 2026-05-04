/**
 * ToastContainer — renders toast notifications in bottom-right corner
 */
import { For } from 'solid-js';
import { toasts, removeToast, type Toast } from '../stores/toast';

function iconForType(type: Toast['type']): string {
    switch (type) {
        case 'success': return 'check_circle';
        case 'error': return 'error';
        case 'warning': return 'warning';
        case 'info': return 'info';
    }
}

export default function ToastContainer() {
    return (
        <div class="toast-container">
            <For each={toasts()}>
                {(toast) => (
                    <div class={`toast toast-${toast.type}`} onClick={() => removeToast(toast.id)}>
                        <span class="material-symbols-rounded toast-icon">{iconForType(toast.type)}</span>
                        <div class="toast-body">
                            <div class="toast-title">{toast.title}</div>
                            {toast.message && <div class="toast-message">{toast.message}</div>}
                        </div>
                        <button class="toast-close" onClick={(e) => { e.stopPropagation(); removeToast(toast.id); }}>
                            <span class="material-symbols-rounded" style="font-size: 16px;">close</span>
                        </button>
                    </div>
                )}
            </For>
        </div>
    );
}

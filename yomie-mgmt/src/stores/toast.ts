/**
 * Toast notification store — reactive toast system for error/success/info messages
 */
import { createSignal } from 'solid-js';

export interface Toast {
    id: number;
    type: 'success' | 'error' | 'warning' | 'info';
    title: string;
    message?: string;
    duration: number;
}

let nextId = 1;

const [toasts, setToasts] = createSignal<Toast[]>([]);

export { toasts };

function addToast(type: Toast['type'], title: string, message?: string, duration = 4000): void {
    const id = nextId++;
    const toast: Toast = { id, type, title, message, duration };

    setToasts(prev => {
        // Max 5 toasts visible
        const list = [...prev, toast];
        return list.length > 5 ? list.slice(-5) : list;
    });

    if (duration > 0) {
        setTimeout(() => removeToast(id), duration);
    }
}

export function removeToast(id: number): void {
    setToasts(prev => prev.filter(t => t.id !== id));
}

export function toastSuccess(title: string, message?: string): void {
    addToast('success', title, message);
}

export function toastError(title: string, message?: string): void {
    addToast('error', title, message, 6000);
}

export function toastWarning(title: string, message?: string): void {
    addToast('warning', title, message);
}

export function toastInfo(title: string, message?: string): void {
    addToast('info', title, message);
}

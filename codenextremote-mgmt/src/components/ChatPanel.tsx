/**
 * ChatPanel — operator chat with device users
 *
 * Uses Tauri IPC commands to communicate with the Rust ChatService backend.
 * ChatService maintains a persistent WebSocket connection to the server.
 * Shows contact list on the left, conversation on the right.
 */
import { createSignal, onMount, onCleanup, Show, For } from 'solid-js';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { t } from '../lib/i18n';
import { toastError } from '../stores/toast';

interface ChatMessage {
    id: number;
    from: string;
    to?: string;
    conversation_id: string;
    text: string;
    timestamp: number;
    read: boolean;
    sent?: boolean;
}

interface ChatContact {
    id: string;
    name: string;
    hostname: string;
    online: boolean;
    last_seen: number;
    unread: number;
    avatar_color: string;
}

interface ChatStatus {
    connected: boolean;
    unread_count: number;
    messages: ChatMessage[];
    contacts: ChatContact[];
    groups: unknown[];
}

export default function ChatPanel() {
    const [contacts, setContacts] = createSignal<ChatContact[]>([]);
    const [activeContact, setActiveContact] = createSignal<string | null>(null);
    const [messages, setMessages] = createSignal<ChatMessage[]>([]);
    const [messageText, setMessageText] = createSignal('');
    const [loading, setLoading] = createSignal(true);
    const [connected, setConnected] = createSignal(false);
    const [typing, setTyping] = createSignal<string | null>(null);
    let messagesEndRef: HTMLDivElement | undefined;
    const unlisteners: UnlistenFn[] = [];

    onMount(async () => {
        // Listen Tauri events from ChatService
        unlisteners.push(await listen<ChatStatus>('chat-status', (e) => {
            setConnected(e.payload.connected);
            setContacts(e.payload.contacts);
        }));
        unlisteners.push(await listen<ChatMessage>('chat-message', (e) => {
            const msg = e.payload;
            setMessages(prev => [...prev, msg]);
            scrollToBottom();
        }));
        unlisteners.push(await listen<ChatContact[]>('chat-contacts', (e) => {
            setContacts(e.payload);
        }));
        unlisteners.push(await listen<ChatMessage[]>('chat-history', (e) => {
            setMessages(e.payload);
            scrollToBottom();
        }));
        unlisteners.push(await listen<string>('chat-typing', (e) => {
            setTyping(e.payload);
            setTimeout(() => setTyping(null), 3000);
        }));
        unlisteners.push(await listen<number>('chat-unread', () => {
            // Refresh contacts to update unread counts
            refreshContacts();
        }));

        // Get initial status from ChatService
        await loadInitialState();
    });

    onCleanup(() => {
        unlisteners.forEach(fn => fn());
    });

    async function loadInitialState() {
        setLoading(true);
        try {
            const status = await invoke<ChatStatus>('get_chat_status');
            setConnected(status.connected);
            setContacts(status.contacts);
            setMessages(status.messages);
            if (!status.connected) {
                // Try reconnecting
                await invoke('reconnect_chat').catch(() => {});
            }
        } catch {
            // ChatService may not be started yet
        } finally {
            setLoading(false);
        }
    }

    async function refreshContacts() {
        try {
            const res = await invoke<{ contacts: ChatContact[] }>('get_chat_contacts');
            if (res?.contacts) setContacts(res.contacts);
        } catch { /* ignore */ }
    }

    async function selectContact(id: string) {
        setActiveContact(id);
        setMessages([]);
        try {
            await invoke('load_chat_conversation', { conversationId: id });
            await invoke('mark_chat_read', { conversationId: id });
        } catch { /* ignore */ }
    }

    function scrollToBottom() {
        setTimeout(() => messagesEndRef?.scrollIntoView({ behavior: 'smooth' }), 50);
    }

    async function sendMessage() {
        const text = messageText().trim();
        const contact = activeContact();
        if (!text || !contact) return;

        try {
            await invoke('send_chat_message', { text, conversationId: contact });
            setMessageText('');
            scrollToBottom();
        } catch {
            toastError(t('common.error'), t('chat.send_error'));
        }
    }

    async function reconnect() {
        try {
            await invoke('reconnect_chat');
        } catch {
            toastError(t('common.error'), t('chat.connection_error'));
        }
    }

    function formatTime(ts: number): string {
        if (!ts) return '';
        return new Date(ts).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }

    return (
        <div class="chat-panel page-enter">
            {/* Contact List */}
            <div class="chat-sidebar">
                <div class="chat-sidebar-header">
                    <span class="material-symbols-rounded" style="font-size: 18px;">chat</span>
                    <span>{t('chat.title')}</span>
                    <Show when={!connected()}>
                        <button class="btn-icon" onClick={reconnect} title={t('chat.reconnect')} style="margin-left: auto;">
                            <span class="material-symbols-rounded" style="font-size: 16px;">refresh</span>
                        </button>
                    </Show>
                    <span class={`status-dot ${connected() ? 'online' : 'offline'}`} style="margin-left: auto;" />
                </div>
                <div class="chat-contact-list">
                    <Show when={!loading()} fallback={<div class="loading-center"><div class="spinner" /></div>}>
                        <Show when={contacts().length > 0} fallback={
                            <div class="empty-state" style="padding: 24px;">
                                <span class="material-symbols-rounded" style="font-size: 32px;">devices_off</span>
                                <div class="empty-state-text">{t('chat.no_contacts')}</div>
                            </div>
                        }>
                            <For each={contacts()}>
                                {(contact) => (
                                    <button
                                        class={`chat-contact ${activeContact() === contact.id ? 'active' : ''}`}
                                        onClick={() => selectContact(contact.id)}
                                    >
                                        <div class="chat-contact-avatar" style={contact.avatar_color ? `background: ${contact.avatar_color}` : undefined}>
                                            {(contact.name || contact.id).charAt(0).toUpperCase()}
                                        </div>
                                        <div class="chat-contact-info">
                                            <div class="chat-contact-name">{contact.name || contact.id}</div>
                                            <div class="chat-contact-id">{contact.id}</div>
                                        </div>
                                        <Show when={contact.unread > 0}>
                                            <span class="notif-badge">{contact.unread}</span>
                                        </Show>
                                        <span class={`status-dot ${contact.online ? 'online' : 'offline'}`} />
                                    </button>
                                )}
                            </For>
                        </Show>
                    </Show>
                </div>
            </div>

            {/* Conversation */}
            <div class="chat-main">
                <Show when={activeContact()} fallback={
                    <div class="empty-state" style="flex: 1;">
                        <span class="material-symbols-rounded">forum</span>
                        <div class="empty-state-text">{t('chat.select_contact')}</div>
                    </div>
                }>
                    <div class="chat-messages">
                        <For each={messages()}>
                            {(msg) => (
                                <div class={`chat-message ${msg.from === 'me' || msg.sent ? 'sent' : 'received'}`}>
                                    <div class="chat-bubble">
                                        <div class="chat-text">{msg.text}</div>
                                        <div class="chat-time">{formatTime(msg.timestamp)}</div>
                                    </div>
                                </div>
                            )}
                        </For>
                        <Show when={typing()}>
                            <div class="chat-message received">
                                <div class="chat-bubble" style="opacity: 0.6; font-style: italic;">
                                    <div class="chat-text">{typing()} {t('chat.is_typing')}</div>
                                </div>
                            </div>
                        </Show>
                        <div ref={messagesEndRef} />
                    </div>
                    <div class="chat-input-bar">
                        <input
                            type="text"
                            class="form-input"
                            placeholder={t('chat.type_message')}
                            value={messageText()}
                            onInput={(e) => setMessageText(e.currentTarget.value)}
                            onKeyDown={(e) => e.key === 'Enter' && sendMessage()}
                            style="flex: 1;"
                        />
                        <button class="btn-primary" style="width: auto; padding: 8px 16px;" onClick={sendMessage}>
                            <span class="material-symbols-rounded" style="font-size: 18px;">send</span>
                        </button>
                    </div>
                </Show>
            </div>
        </div>
    );
}

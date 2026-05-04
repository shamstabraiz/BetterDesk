//! Instant chat service — persistent WebSocket connection to the Yomie
//! server chat endpoint.
//!
//! Server endpoint: `ws://<host>:<port>/ws/chat/<device_id>`
//!
//! Protocol (JSON frames):
//!   Client → Server:
//!     { "type": "message", "text": "hello", "device_id": "ABC" }
//!
//!   Server → Client:
//!     { "type": "message",  "from": "operator", "text": "hello", "timestamp": 1234567890 }
//!     { "type": "history",  "messages": [ ... ] }
//!     { "type": "typing",   "from": "operator" }
//!     { "type": "read",     "message_id": 42 }

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::{sync::mpsc, time::Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

// ---------------------------------------------------------------------------
//  Types
// ---------------------------------------------------------------------------

/// A single chat message (stored in-memory and emitted to the frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: u64,
    pub from: String,   // "operator" | "agent" | "system" | <device_id>
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub conversation_id: String,
    pub text: String,
    pub timestamp: i64, // Unix ms
    pub read: bool,
}

/// A chat contact discovered from the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatContact {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub online: bool,
    pub last_seen: i64,
    pub unread: u32,
    pub avatar_color: String,
}

/// A chat group for multi-device conversations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatGroup {
    pub id: String,
    pub name: String,
    pub members: Vec<String>,
    pub created_by: String,
    pub unread: u32,
}

/// Frontend-facing service status snapshot.
#[derive(Debug, Clone, Serialize)]
pub struct ChatStatus {
    pub connected: bool,
    pub unread_count: u32,
    pub messages: Vec<ChatMessage>,
    pub contacts: Vec<ChatContact>,
    pub groups: Vec<ChatGroup>,
}

/// Frames sent from the frontend to the background task.
#[derive(Debug)]
pub enum ChatCommand {
    Send(String, Option<String>), // text, optional conversation_id
    MarkRead(Option<String>),     // optional conversation_id
    LoadConversation(String),     // conversation_id
    FetchContacts,
    CreateGroup(String, Vec<String>), // name, member_ids
    Stop,
}

// ---------------------------------------------------------------------------
//  Service handle
// ---------------------------------------------------------------------------

pub struct ChatService {
    sender: mpsc::Sender<ChatCommand>,
    messages: Arc<Mutex<Vec<ChatMessage>>>,
    contacts: Arc<Mutex<Vec<ChatContact>>>,
    groups: Arc<Mutex<Vec<ChatGroup>>>,
    unread: Arc<AtomicU32>,
    connected: Arc<AtomicBool>,
}

impl ChatService {
    /// Start the background WebSocket chat task.
    pub fn start<R: Runtime>(
        app: AppHandle<R>,
        ws_url: String,
        device_id: String,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<ChatCommand>(128);
        let messages = Arc::new(Mutex::new(Vec::<ChatMessage>::new()));
        let contacts = Arc::new(Mutex::new(Vec::<ChatContact>::new()));
        let groups = Arc::new(Mutex::new(Vec::<ChatGroup>::new()));
        let unread = Arc::new(AtomicU32::new(0));
        let connected = Arc::new(AtomicBool::new(false));

        let msgs_clone = messages.clone();
        let contacts_clone = contacts.clone();
        let groups_clone = groups.clone();
        let unread_clone = unread.clone();
        let conn_clone = connected.clone();

        tauri::async_runtime::spawn(async move {
            chat_loop(
                app, rx, ws_url, device_id,
                msgs_clone, contacts_clone, groups_clone,
                unread_clone, conn_clone,
            ).await;
        });

        ChatService { sender: tx, messages, contacts, groups, unread, connected }
    }

    /// Send a message, optionally to a specific conversation.
    pub fn send(&self, text: String, conversation_id: Option<String>) {
        let _ = self.sender.try_send(ChatCommand::Send(text, conversation_id));
    }

    /// Mark messages as read, optionally for a specific conversation.
    pub fn mark_read(&self, conversation_id: Option<String>) {
        let _ = self.sender.try_send(ChatCommand::MarkRead(conversation_id));
    }

    /// Request contact list refresh.
    pub fn fetch_contacts(&self) {
        let _ = self.sender.try_send(ChatCommand::FetchContacts);
    }

    /// Load history for a specific conversation.
    pub fn load_conversation(&self, conversation_id: String) {
        let _ = self.sender.try_send(ChatCommand::LoadConversation(conversation_id));
    }

    /// Create a new chat group.
    pub fn create_group(&self, name: String, member_ids: Vec<String>) {
        let _ = self.sender.try_send(ChatCommand::CreateGroup(name, member_ids));
    }

    /// Stop the service.
    pub fn stop(&self) {
        let _ = self.sender.try_send(ChatCommand::Stop);
    }

    /// Snapshot of current service status.
    pub fn status(&self) -> ChatStatus {
        ChatStatus {
            connected: self.connected.load(Ordering::Relaxed),
            unread_count: self.unread.load(Ordering::Relaxed),
            messages: self.messages.lock().unwrap().clone(),
            contacts: self.contacts.lock().unwrap().clone(),
            groups: self.groups.lock().unwrap().clone(),
        }
    }
}

// ---------------------------------------------------------------------------
//  Background loop
// ---------------------------------------------------------------------------

async fn chat_loop<R: Runtime>(
    app: AppHandle<R>,
    mut rx: mpsc::Receiver<ChatCommand>,
    ws_url: String,
    device_id: String,
    messages: Arc<Mutex<Vec<ChatMessage>>>,
    contacts: Arc<Mutex<Vec<ChatContact>>>,
    groups: Arc<Mutex<Vec<ChatGroup>>>,
    unread: Arc<AtomicU32>,
    connected: Arc<AtomicBool>,
) {
    let mut retry_delay = Duration::from_secs(3);
    let max_delay = Duration::from_secs(60);

    loop {
        info!("Chat: connecting to {}", ws_url);

        match tokio::time::timeout(
            Duration::from_secs(15),
            connect_async(&ws_url),
        )
        .await
        {
            Ok(Ok((ws_stream, _))) => {
                retry_delay = Duration::from_secs(3);
                connected.store(true, Ordering::Relaxed);
                emit_status(&app, &connected, &unread, &messages, &contacts, &groups);

                info!("Chat: connected");

                let (mut write, mut read) = ws_stream.split();

                // Send initial hello
                let hello = serde_json::json!({
                    "type": "hello",
                    "device_id": device_id,
                    "capabilities": ["multi_conversation", "contacts", "groups"],
                });
                if let Ok(txt) = serde_json::to_string(&hello) {
                    let _ = write.send(Message::Text(txt.into())).await;
                }

                // Request contact list on connect
                let contact_req = serde_json::json!({
                    "type": "get_contacts",
                    "device_id": device_id,
                });
                if let Ok(txt) = serde_json::to_string(&contact_req) {
                    let _ = write.send(Message::Text(txt.into())).await;
                }

                loop {
                    tokio::select! {
                        // Message from server
                        msg = read.next() => {
                            match msg {
                                Some(Ok(Message::Text(txt))) => {
                                    handle_server_frame(
                                        &app, &txt, &messages, &contacts, &groups,
                                        &unread, &connected,
                                    );
                                }
                                Some(Ok(Message::Close(_))) | None => {
                                    warn!("Chat: server closed connection");
                                    break;
                                }
                                Some(Err(e)) => {
                                    error!("Chat WebSocket error: {}", e);
                                    break;
                                }
                                _ => {}
                            }
                        }
                        // Command from frontend
                        cmd = rx.recv() => {
                            match cmd {
                                Some(ChatCommand::Send(text, conv_id)) => {
                                    let outgoing = serde_json::json!({
                                        "type": "message",
                                        "device_id": device_id,
                                        "text": text,
                                        "conversation_id": conv_id.as_deref().unwrap_or("operator"),
                                        "timestamp": Utc::now().timestamp_millis(),
                                    });
                                    if let Ok(txt) = serde_json::to_string(&outgoing) {
                                        if write.send(Message::Text(txt.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                    // Add to local messages
                                    let msg = ChatMessage {
                                        id: next_id(&messages),
                                        from: "agent".into(),
                                        to: conv_id.clone(),
                                        conversation_id: conv_id.unwrap_or_else(|| "operator".into()),
                                        text: text.clone(),
                                        timestamp: Utc::now().timestamp_millis(),
                                        read: true,
                                    };
                                    messages.lock().unwrap().push(msg.clone());
                                    let _ = app.emit("chat-message", &msg);
                                }
                                Some(ChatCommand::MarkRead(conv_id)) => {
                                    if conv_id.is_none() {
                                        unread.store(0, Ordering::Relaxed);
                                        let mut msgs = messages.lock().unwrap();
                                        for m in msgs.iter_mut() { m.read = true; }
                                    } else {
                                        let cid = conv_id.unwrap();
                                        let mut msgs = messages.lock().unwrap();
                                        for m in msgs.iter_mut() {
                                            if m.conversation_id == cid { m.read = true; }
                                        }
                                        // Recalculate unread
                                        let count = msgs.iter().filter(|m| !m.read && m.from != "agent").count();
                                        unread.store(count as u32, Ordering::Relaxed);
                                    }
                                    emit_status(&app, &connected, &unread, &messages, &contacts, &groups);
                                }
                                Some(ChatCommand::LoadConversation(conv_id)) => {
                                    let req = serde_json::json!({
                                        "type": "get_history",
                                        "device_id": device_id,
                                        "conversation_id": conv_id,
                                    });
                                    if let Ok(txt) = serde_json::to_string(&req) {
                                        let _ = write.send(Message::Text(txt.into())).await;
                                    }
                                }
                                Some(ChatCommand::FetchContacts) => {
                                    let req = serde_json::json!({
                                        "type": "get_contacts",
                                        "device_id": device_id,
                                    });
                                    if let Ok(txt) = serde_json::to_string(&req) {
                                        let _ = write.send(Message::Text(txt.into())).await;
                                    }
                                }
                                Some(ChatCommand::CreateGroup(name, member_ids)) => {
                                    let req = serde_json::json!({
                                        "type": "create_group",
                                        "device_id": device_id,
                                        "name": name,
                                        "member_ids": member_ids,
                                    });
                                    if let Ok(txt) = serde_json::to_string(&req) {
                                        let _ = write.send(Message::Text(txt.into())).await;
                                    }
                                }
                                Some(ChatCommand::Stop) | None => {
                                    let _ = write.close().await;
                                    connected.store(false, Ordering::Relaxed);
                                    return;
                                }
                            }
                        }
                    }
                }

                connected.store(false, Ordering::Relaxed);
                emit_status(&app, &connected, &unread, &messages, &contacts, &groups);
            }
            Ok(Err(e)) => {
                error!("Chat: connection failed: {}", e);
            }
            Err(_) => {
                error!("Chat: connection timed out (15s)");
            }
        }

        // Drain stop commands while waiting to retry
        tokio::select! {
            _ = tokio::time::sleep(retry_delay) => {}
            cmd = rx.recv() => {
                if matches!(cmd, Some(ChatCommand::Stop) | None) {
                    return;
                }
            }
        }

        retry_delay = (retry_delay * 2).min(max_delay);
        info!("Chat: retrying in {:?}", retry_delay);
    }
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

fn handle_server_frame<R: Runtime>(
    app: &AppHandle<R>,
    raw: &str,
    messages: &Arc<Mutex<Vec<ChatMessage>>>,
    contacts: &Arc<Mutex<Vec<ChatContact>>>,
    groups: &Arc<Mutex<Vec<ChatGroup>>>,
    unread: &Arc<AtomicU32>,
    _connected: &Arc<AtomicBool>,
) {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        debug!("Chat: unparseable frame: {}", raw);
        return;
    };

    match val.get("type").and_then(|v| v.as_str()) {
        Some("message") => {
            let msg = ChatMessage {
                id: val.get("id").and_then(|v| v.as_u64()).unwrap_or_else(|| next_id(messages)),
                from: val.get("from").and_then(|v| v.as_str()).unwrap_or("operator").to_string(),
                to: val.get("to").and_then(|v| v.as_str()).map(|s| s.to_string()),
                conversation_id: val.get("conversation_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("operator")
                    .to_string(),
                text: val.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                timestamp: val.get("timestamp").and_then(|v| v.as_i64())
                    .unwrap_or_else(|| Utc::now().timestamp_millis()),
                read: false,
            };
            unread.fetch_add(1, Ordering::Relaxed);
            messages.lock().unwrap().push(msg.clone());
            let _ = app.emit("chat-message", &msg);
            let _ = app.emit("chat-unread", unread.load(Ordering::Relaxed));
        }
        Some("history") => {
            if let Some(arr) = val.get("messages").and_then(|v| v.as_array()) {
                let mut msgs = messages.lock().unwrap();
                // If conversation-specific, only replace those messages
                let conv_id = val.get("conversation_id").and_then(|v| v.as_str());
                if let Some(cid) = conv_id {
                    msgs.retain(|m| m.conversation_id != cid);
                } else {
                    msgs.clear();
                }
                for item in arr {
                    if let Ok(m) = serde_json::from_value::<ChatMessage>(item.clone()) {
                        msgs.push(m);
                    }
                }
                let _ = app.emit("chat-history", &*msgs);
            }
        }
        Some("contacts") => {
            if let Some(arr) = val.get("contacts").and_then(|v| v.as_array()) {
                let mut contact_list = contacts.lock().unwrap();
                contact_list.clear();
                for item in arr {
                    if let Ok(c) = serde_json::from_value::<ChatContact>(item.clone()) {
                        contact_list.push(c);
                    }
                }
                let _ = app.emit("chat-contacts", &*contact_list);
            }
        }
        Some("groups") | Some("group_list") => {
            if let Some(arr) = val.get("groups").and_then(|v| v.as_array()) {
                let mut group_list = groups.lock().unwrap();
                group_list.clear();
                for item in arr {
                    if let Ok(g) = serde_json::from_value::<ChatGroup>(item.clone()) {
                        group_list.push(g);
                    }
                }
            }
        }
        Some("group_created") => {
            if let Ok(g) = serde_json::from_value::<ChatGroup>(val.clone()) {
                groups.lock().unwrap().push(g);
            }
        }
        Some("typing") => {
            let _ = app.emit("chat-typing", val.get("from").and_then(|v| v.as_str()).unwrap_or("operator"));
        }
        Some("read") => {
            if let Some(id) = val.get("message_id").and_then(|v| v.as_u64()) {
                let mut msgs = messages.lock().unwrap();
                for m in msgs.iter_mut() {
                    if m.id == id { m.read = true; }
                }
            }
        }
        Some("presence") => {
            // Update contact online status
            if let (Some(contact_id), Some(online)) = (
                val.get("device_id").and_then(|v| v.as_str()),
                val.get("online").and_then(|v| v.as_bool()),
            ) {
                let mut contact_list = contacts.lock().unwrap();
                if let Some(c) = contact_list.iter_mut().find(|c| c.id == contact_id) {
                    c.online = online;
                    if !online {
                        c.last_seen = Utc::now().timestamp_millis();
                    }
                }
                let _ = app.emit("chat-contacts", &*contact_list);
            }
        }
        Some("welcome") => {
            // Server acknowledged our hello — connection is fully established
            info!("Chat: server welcome received");
        }
        Some("status") => {
            // Agent connected/disconnected notification from server
            if let Some(agent_connected) = val.get("agent_connected").and_then(|v| v.as_bool()) {
                debug!("Chat: agent_connected = {}", agent_connected);
            }
        }
        other => {
            debug!("Chat: unhandled frame type: {:?}", other);
        }
    }
}

fn next_id(messages: &Arc<Mutex<Vec<ChatMessage>>>) -> u64 {
    messages.lock().unwrap().len() as u64 + 1
}

fn emit_status<R: Runtime>(
    app: &AppHandle<R>,
    connected: &Arc<AtomicBool>,
    unread: &Arc<AtomicU32>,
    messages: &Arc<Mutex<Vec<ChatMessage>>>,
    contacts: &Arc<Mutex<Vec<ChatContact>>>,
    groups: &Arc<Mutex<Vec<ChatGroup>>>,
) {
    let status = ChatStatus {
        connected: connected.load(Ordering::Relaxed),
        unread_count: unread.load(Ordering::Relaxed),
        messages: messages.lock().unwrap().clone(),
        contacts: contacts.lock().unwrap().clone(),
        groups: groups.lock().unwrap().clone(),
    };
    let _ = app.emit("chat-status", &status);
}

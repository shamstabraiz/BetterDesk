//! CDAP wire protocol — message envelope and payload types.
//!
//! Mirrors the Go-side `cdap.Message` struct used by both the gateway
//! and the reference agent (`yomie-agent`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Top-level CDAP message envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdapMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(flatten)]
    pub payload: MessagePayload,
}

/// All known payload variants.
///
/// Unknown types are captured in `Generic` to avoid breaking on protocol
/// extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessagePayload {
    // --- Authentication ---
    Auth {
        #[serde(skip_serializing_if = "Option::is_none")]
        method: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        token: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        username: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        password: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        device_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        client_version: Option<String>,
    },

    AuthResult {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        token: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        role: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        device_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        session_token: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        requires_2fa: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    // --- Registration ---
    Register {
        manifest: serde_json::Value,
    },

    // --- Heartbeat ---
    Heartbeat {
        metrics: SystemMetrics,
        #[serde(skip_serializing_if = "Option::is_none")]
        widget_values: Option<std::collections::HashMap<String, Value>>,
    },

    // --- Commands ---
    Command {
        command_id: String,
        widget_id: String,
        action: String,
        #[serde(default)]
        value: Value,
    },

    CommandResponse {
        command_id: String,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    // --- Widget state ---
    StateUpdate {
        widget_values: std::collections::HashMap<String, Value>,
    },

    // --- Terminal ---
    TerminalStart {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        shell: Option<String>,
        #[serde(default = "default_cols")]
        cols: u16,
        #[serde(default = "default_rows")]
        rows: u16,
    },

    TerminalInput {
        session_id: String,
        data: String,
    },

    TerminalOutput {
        session_id: String,
        data: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        stream: Option<String>,
    },

    TerminalResize {
        session_id: String,
        cols: u16,
        rows: u16,
    },

    TerminalKill {
        session_id: String,
    },

    TerminalEnd {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },

    // --- File browser ---
    FileList {
        file_id: String,
        path: String,
    },

    FileListResponse {
        file_id: String,
        entries: Vec<FileEntry>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    FileRead {
        file_id: String,
        path: String,
        #[serde(default)]
        offset: u64,
        #[serde(default = "default_chunk_size")]
        length: u64,
    },

    FileReadResponse {
        file_id: String,
        data: String, // base64
        eof: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    FileWrite {
        file_id: String,
        path: String,
        data: String, // base64
        #[serde(default)]
        offset: u64,
    },

    FileWriteResponse {
        file_id: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    FileDelete {
        file_id: String,
        path: String,
    },

    FileDeleteResponse {
        file_id: String,
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },

    // --- Clipboard ---
    ClipboardUpdate {
        format: String,
        data: String,
    },

    ClipboardSet {
        format: String,
        data: String,
    },

    // --- Screenshot ---
    ScreenshotResponse {
        format: String,
        data: String, // base64
        width: u32,
        height: u32,
    },

    // --- Desktop (remote) ---
    DesktopStart {
        session_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        codec: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        quality: Option<String>,
        #[serde(default = "default_fps")]
        fps: u32,
    },

    DesktopFrame {
        session_id: String,
        data: String, // base64 encoded frame
        format: String,
        timestamp: u64,
        width: u32,
        height: u32,
    },

    DesktopInput {
        session_id: String,
        input_type: String, // "mouse_move", "mouse_down", "mouse_up", "key_down", "key_up", "scroll"
        #[serde(flatten)]
        params: Value,
    },

    DesktopStop {
        session_id: String,
    },

    // --- Error ---
    Error {
        code: i32,
        message: String,
    },

    // --- Catch-all ---
    Generic(Value),
}

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}
fn default_chunk_size() -> u64 {
    1_048_576 // 1 MB
}
fn default_fps() -> u32 {
    15
}

// ---------------------------------------------------------------------------
//  Shared sub-types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemMetrics {
    #[serde(default)]
    pub cpu: f64,
    #[serde(default)]
    pub memory: f64,
    #[serde(default)]
    pub disk: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
}

// ---------------------------------------------------------------------------
//  Manifest types (used during registration)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceManifest {
    pub device: DeviceDescriptor,
    pub capabilities: Vec<String>,
    pub widgets: Vec<Widget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alerts: Option<Vec<AlertDef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heartbeat_interval: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDescriptor {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Widget {
    pub id: String,
    #[serde(rename = "type")]
    pub widget_type: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub readonly: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<TableColumn>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<SelectOption>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dangerous: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableColumn {
    pub key: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sortable: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertDef {
    pub id: String,
    pub severity: String,
    pub condition: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_resolve: Option<bool>,
}

// ---------------------------------------------------------------------------
//  Message construction helpers
// ---------------------------------------------------------------------------

impl CdapMessage {
    pub fn auth(
        method: &str,
        device_id: &str,
        key: Option<&str>,
        token: Option<&str>,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Self {
        Self {
            msg_type: "auth".into(),
            payload: MessagePayload::Auth {
                method: Some(method.into()),
                key: key.map(|s| s.into()),
                token: token.map(|s| s.into()),
                username: username.map(|s| s.into()),
                password: password.map(|s| s.into()),
                device_id: Some(device_id.into()),
                client_version: Some(env!("CARGO_PKG_VERSION").into()),
            },
        }
    }

    pub fn register(manifest: &DeviceManifest) -> Self {
        Self {
            msg_type: "register".into(),
            payload: MessagePayload::Register {
                manifest: serde_json::to_value(manifest).unwrap_or_default(),
            },
        }
    }

    pub fn heartbeat(
        metrics: SystemMetrics,
        widget_values: Option<std::collections::HashMap<String, Value>>,
    ) -> Self {
        Self {
            msg_type: "heartbeat".into(),
            payload: MessagePayload::Heartbeat {
                metrics,
                widget_values,
            },
        }
    }

    pub fn command_response(
        command_id: &str,
        status: &str,
        result: Option<Value>,
        error: Option<String>,
    ) -> Self {
        Self {
            msg_type: "command_response".into(),
            payload: MessagePayload::CommandResponse {
                command_id: command_id.into(),
                status: status.into(),
                result,
                error,
            },
        }
    }

    pub fn state_update(values: std::collections::HashMap<String, Value>) -> Self {
        Self {
            msg_type: "state_update".into(),
            payload: MessagePayload::StateUpdate {
                widget_values: values,
            },
        }
    }

    pub fn terminal_output(session_id: &str, data: &str) -> Self {
        Self {
            msg_type: "terminal_output".into(),
            payload: MessagePayload::TerminalOutput {
                session_id: session_id.into(),
                data: data.into(),
                stream: Some("stdout".into()),
            },
        }
    }

    pub fn terminal_end(session_id: &str, exit_code: Option<i32>) -> Self {
        Self {
            msg_type: "terminal_end".into(),
            payload: MessagePayload::TerminalEnd {
                session_id: session_id.into(),
                exit_code,
            },
        }
    }

    pub fn pong() -> Self {
        Self {
            msg_type: "pong".into(),
            payload: MessagePayload::Generic(serde_json::json!({
                "timestamp": chrono::Utc::now().timestamp_millis()
            })),
        }
    }
}

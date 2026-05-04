//! CDAP (Custom Device Automation Protocol) desktop agent.
//!
//! Connects this Windows machine to the Yomie CDAP gateway as a
//! fully-managed "desktop" device, exposing:
//! - System telemetry (CPU, RAM, Disk, Network)
//! - Remote terminal (cmd.exe / PowerShell)
//! - Remote desktop (screen capture + input relay)
//! - File browser (full CRUD with security)
//! - Clipboard sync
//! - System management (services, processes, users, firewall, registry)
//! - Automation (script execution, scheduled tasks)
//! - Security monitoring (Defender, audit logs, firewall rules)
//! - Network management (interfaces, DNS, routing)

pub mod automation;
pub mod connection;
pub mod desktop;
pub mod files;
pub mod heartbeat;
pub mod manifest;
pub mod network_mgmt;
pub mod protocol;
pub mod security;
pub mod sysmanage;
pub mod terminal;

use anyhow::Result;
use log::{error, info, warn};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};

use connection::{CdapConnection, CdapSender};
use manifest::build_manifest;
use protocol::{CdapMessage, MessagePayload};

// ---------------------------------------------------------------------------
//  Public types
// ---------------------------------------------------------------------------

/// CDAP agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdapConfig {
    /// WebSocket URL, e.g. "ws://192.168.0.110:21122/cdap"
    pub gateway_url: String,
    /// Authentication method: "api_key", "device_token", or "user_password"
    pub auth_method: String,
    /// API key (when auth_method = "api_key")
    pub api_key: String,
    /// Device token (when auth_method = "device_token")
    pub device_token: String,
    /// Username (when auth_method = "user_password")
    pub username: String,
    /// Password (when auth_method = "user_password")
    pub password: String,
    /// Unique device identifier (auto-generated if empty)
    pub device_id: String,
    /// Friendly device name (defaults to hostname)
    pub device_name: String,
    /// Root path for file browser (default: "C:\\")
    pub file_root: String,
    /// Enable terminal access
    pub enable_terminal: bool,
    /// Enable file browser
    pub enable_file_browser: bool,
    /// Enable clipboard sync
    pub enable_clipboard: bool,
    /// Enable remote desktop
    pub enable_remote_desktop: bool,
    /// Enable system management commands
    pub enable_sysmanage: bool,
    /// Enable automation engine
    pub enable_automation: bool,
    /// Heartbeat interval in seconds
    pub heartbeat_interval_secs: u64,
    /// Auto-connect on startup
    pub auto_connect: bool,
}

impl Default for CdapConfig {
    fn default() -> Self {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "Windows-Desktop".into());

        Self {
            gateway_url: String::new(),
            auth_method: "api_key".into(),
            api_key: String::new(),
            device_token: String::new(),
            username: String::new(),
            password: String::new(),
            device_id: String::new(),
            device_name: hostname,
            file_root: "C:\\".into(),
            enable_terminal: true,
            enable_file_browser: true,
            enable_clipboard: true,
            enable_remote_desktop: true,
            enable_sysmanage: true,
            enable_automation: true,
            heartbeat_interval_secs: 15,
            auto_connect: false,
        }
    }
}

impl CdapConfig {
    /// Load CDAP configuration from disk.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            let config: CdapConfig = serde_json::from_str(&data)?;
            Ok(config)
        } else {
            Ok(CdapConfig::default())
        }
    }

    /// Save CDAP configuration to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data)?;
        Ok(())
    }

    fn config_path() -> Result<std::path::PathBuf> {
        let dir = directories::ProjectDirs::from("com", "yomie", "Yomie")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        Ok(dir.join("cdap_config.json"))
    }
}

/// Runtime status of the CDAP agent.
#[derive(Debug, Clone, Serialize)]
pub struct CdapStatus {
    pub connected: bool,
    pub device_id: String,
    pub gateway_url: String,
    pub uptime_secs: u64,
    pub heartbeat_count: u64,
    pub active_sessions: Vec<String>,
    pub last_error: Option<String>,
}

// ---------------------------------------------------------------------------
//  Agent
// ---------------------------------------------------------------------------

/// The CDAP desktop agent. Call `start()` to launch, `stop()` to tear down.
pub struct CdapAgent {
    running: Arc<AtomicBool>,
    status_rx: watch::Receiver<CdapStatus>,
    cmd_tx: mpsc::Sender<AgentCommand>,
    _join_handle: tauri::async_runtime::JoinHandle<()>,
}

enum AgentCommand {
    Stop,
    #[allow(dead_code)]
    SendCommand {
        widget_id: String,
        action: String,
        value: serde_json::Value,
    },
}

impl CdapAgent {
    /// Start the CDAP agent with the given configuration.
    pub fn start(config: CdapConfig, app_handle: tauri::AppHandle) -> Self {
        let running = Arc::new(AtomicBool::new(true));
        let running2 = running.clone();

        let initial_status = CdapStatus {
            connected: false,
            device_id: config.device_id.clone(),
            gateway_url: config.gateway_url.clone(),
            uptime_secs: 0,
            heartbeat_count: 0,
            active_sessions: vec![],
            last_error: None,
        };

        let (status_tx, status_rx) = watch::channel(initial_status);
        let (cmd_tx, cmd_rx) = mpsc::channel::<AgentCommand>(64);

        let join_handle = tauri::async_runtime::spawn(async move {
            agent_loop(config, running2, status_tx, cmd_rx, app_handle).await;
        });

        Self {
            running,
            status_rx,
            cmd_tx,
            _join_handle: join_handle,
        }
    }

    /// Get the current agent status.
    pub fn status(&self) -> CdapStatus {
        self.status_rx.borrow().clone()
    }

    /// Stop the agent gracefully.
    pub fn stop(&self) {
        self.running.store(false, Ordering::Relaxed);
        let _ = self.cmd_tx.try_send(AgentCommand::Stop);
    }
}

// ---------------------------------------------------------------------------
//  Main loop
// ---------------------------------------------------------------------------

async fn agent_loop(
    mut config: CdapConfig,
    running: Arc<AtomicBool>,
    status_tx: watch::Sender<CdapStatus>,
    mut cmd_rx: mpsc::Receiver<AgentCommand>,
    app_handle: tauri::AppHandle,
) {
    let start_time = std::time::Instant::now();
    let mut heartbeat_count: u64 = 0;
    let mut reconnect_delay = std::time::Duration::from_secs(2);
    let max_reconnect_delay = std::time::Duration::from_secs(60);

    while running.load(Ordering::Relaxed) {
        // Skip connection when API key is empty — auth will always fail.
        if config.api_key.is_empty() {
            warn!("CDAP: No API key configured — waiting for key to be set");
            let _ = status_tx.send(CdapStatus {
                connected: false,
                device_id: config.device_id.clone(),
                gateway_url: config.gateway_url.clone(),
                uptime_secs: start_time.elapsed().as_secs(),
                heartbeat_count,
                active_sessions: vec![],
                last_error: Some("No API key configured".into()),
            });
            // Wait 60s then re-check (key might be set via IPC)
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            // Reload config from disk to pick up any changes
            if let Ok(fresh) = CdapConfig::load() {
                config = fresh;
            }
            continue;
        }

        info!("CDAP: Connecting to {}", config.gateway_url);

        let _ = status_tx.send(CdapStatus {
            connected: false,
            device_id: config.device_id.clone(),
            gateway_url: config.gateway_url.clone(),
            uptime_secs: start_time.elapsed().as_secs(),
            heartbeat_count,
            active_sessions: vec![],
            last_error: Some("Connecting...".into()),
        });

        match CdapConnection::connect(&config).await {
            Ok(mut conn) => {
                info!("CDAP: Connected, authenticating...");

                // Phase 1: Authenticate
                if let Err(e) = conn.authenticate(&config).await {
                    error!("CDAP: Authentication failed: {}", e);
                    let _ = status_tx.send(CdapStatus {
                        connected: false,
                        device_id: config.device_id.clone(),
                        gateway_url: config.gateway_url.clone(),
                        uptime_secs: start_time.elapsed().as_secs(),
                        heartbeat_count,
                        active_sessions: vec![],
                        last_error: Some(format!("Auth failed: {}", e)),
                    });
                    // Back off on auth failure too (not just connection failure)
                    info!("CDAP: Auth retry in {:?}...", reconnect_delay);
                    tokio::time::sleep(reconnect_delay).await;
                    reconnect_delay = (reconnect_delay * 2).min(max_reconnect_delay);
                    continue;
                }

                info!("CDAP: Authenticated, registering manifest...");

                // Phase 2: Register manifest
                let manifest = build_manifest(&config);
                if let Err(e) = conn.register_manifest(&manifest).await {
                    error!("CDAP: Registration failed: {}", e);
                    let _ = status_tx.send(CdapStatus {
                        connected: false,
                        device_id: config.device_id.clone(),
                        gateway_url: config.gateway_url.clone(),
                        uptime_secs: start_time.elapsed().as_secs(),
                        heartbeat_count,
                        active_sessions: vec![],
                        last_error: Some(format!("Registration failed: {}", e)),
                    });
                    info!("CDAP: Registration retry in {:?}...", reconnect_delay);
                    tokio::time::sleep(reconnect_delay).await;
                    reconnect_delay = (reconnect_delay * 2).min(max_reconnect_delay);
                    continue;
                }

                // Full success — reset backoff
                reconnect_delay = std::time::Duration::from_secs(2);

                info!("CDAP: Registered as {}", config.device_id);

                // Phase 3: Main message loop
                let (sender, mut receiver) = conn.into_split();
                let sender = Arc::new(Mutex::new(sender));
                let mut sessions = SessionManager::new();

                let heartbeat_interval =
                    std::time::Duration::from_secs(config.heartbeat_interval_secs.max(5));
                let mut heartbeat_timer = tokio::time::interval(heartbeat_interval);
                heartbeat_timer.tick().await; // Skip first immediate tick

                let _ = status_tx.send(CdapStatus {
                    connected: true,
                    device_id: config.device_id.clone(),
                    gateway_url: config.gateway_url.clone(),
                    uptime_secs: start_time.elapsed().as_secs(),
                    heartbeat_count,
                    active_sessions: vec![],
                    last_error: None,
                });

                loop {
                    tokio::select! {
                        // Heartbeat tick
                        _ = heartbeat_timer.tick() => {
                            let config2 = config.clone();
                            let metrics = tokio::task::spawn_blocking(move || {
                                heartbeat::collect_metrics(&config2)
                            })
                            .await
                            .unwrap_or_else(|_| heartbeat::collect_metrics_fallback());

                            let mut tx = sender.lock().await;
                            if let Err(e) = tx.send_heartbeat(&metrics).await {
                                warn!("CDAP: Heartbeat send failed: {}", e);
                                break;
                            }
                            drop(tx);
                            heartbeat_count += 1;

                            let _ = status_tx.send(CdapStatus {
                                connected: true,
                                device_id: config.device_id.clone(),
                                gateway_url: config.gateway_url.clone(),
                                uptime_secs: start_time.elapsed().as_secs(),
                                heartbeat_count,
                                active_sessions: sessions.list_active(),
                                last_error: None,
                            });
                        }

                        // Incoming message from gateway (no lock needed — only reader)
                        msg = receiver.recv_message() => {
                            match msg {
                                Ok(Some(message)) => {
                                    if let Err(e) = handle_message(
                                        &message,
                                        &sender,
                                        &config,
                                        &mut sessions,
                                        &app_handle,
                                    ).await {
                                        warn!("CDAP: Message handling error: {}", e);
                                    }
                                }
                                Ok(None) => {
                                    info!("CDAP: Gateway closed connection");
                                    break;
                                }
                                Err(e) => {
                                    error!("CDAP: Receive error: {}", e);
                                    break;
                                }
                            }
                        }

                        // Commands from the frontend
                        cmd = cmd_rx.recv() => {
                            match cmd {
                                Some(AgentCommand::Stop) | None => {
                                    info!("CDAP: Stopping agent");
                                    running.store(false, Ordering::Relaxed);
                                    break;
                                }
                                Some(AgentCommand::SendCommand { widget_id: _, action: _, value }) => {
                                    let response = protocol::CdapMessage {
                                        msg_type: "command_response".into(),
                                        payload: MessagePayload::CommandResponse {
                                            command_id: String::new(),
                                            status: "ok".into(),
                                            result: Some(value),
                                            error: None,
                                        },
                                    };
                                    let mut tx = sender.lock().await;
                                    if let Err(e) = tx.send_message(&response).await {
                                        warn!("CDAP: Failed to send command response: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }

                // Cleanup: close sender
                {
                    let mut tx = sender.lock().await;
                    tx.close().await;
                }

                // Cleanup active sessions
                sessions.close_all().await;
            }
            Err(e) => {
                error!("CDAP: Connection failed: {}", e);
                let _ = status_tx.send(CdapStatus {
                    connected: false,
                    device_id: config.device_id.clone(),
                    gateway_url: config.gateway_url.clone(),
                    uptime_secs: start_time.elapsed().as_secs(),
                    heartbeat_count,
                    active_sessions: vec![],
                    last_error: Some(format!("Connection failed: {}", e)),
                });
            }
        }

        if !running.load(Ordering::Relaxed) {
            break;
        }

        info!("CDAP: Reconnecting in {:?}...", reconnect_delay);
        tokio::time::sleep(reconnect_delay).await;
        reconnect_delay = (reconnect_delay * 2).min(max_reconnect_delay);
    }

    info!("CDAP: Agent loop exited");
}

// ---------------------------------------------------------------------------
//  Message dispatch
// ---------------------------------------------------------------------------

async fn handle_message(
    msg: &CdapMessage,
    sender: &Arc<Mutex<CdapSender>>,
    config: &CdapConfig,
    sessions: &mut SessionManager,
    _app_handle: &tauri::AppHandle,
) -> Result<()> {
    match msg.msg_type.as_str() {
        "command" => {
            if let MessagePayload::Command {
                ref command_id,
                ref widget_id,
                ref action,
                ref value,
            } = msg.payload
            {
                let result = dispatch_command(widget_id, action, value, config, sessions).await;
                let response = CdapMessage {
                    msg_type: "command_response".into(),
                    payload: MessagePayload::CommandResponse {
                        command_id: command_id.clone(),
                        status: if result.is_ok() { "ok" } else { "error" }.into(),
                        result: result.as_ref().ok().cloned(),
                        error: result.err().map(|e| e.to_string()),
                    },
                };
                let mut tx = sender.lock().await;
                tx.send_message(&response).await?;
            }
        }

        "terminal_start" => {
            if let MessagePayload::TerminalStart {
                ref session_id,
                ref shell,
                cols,
                rows,
            } = msg.payload
            {
                if config.enable_terminal {
                    let sender2 = sender.clone();
                    let sid = session_id.clone();
                    let sh = shell.clone().unwrap_or_else(|| "powershell".into());
                    sessions.start_terminal(sid, sh, cols, rows, sender2).await?;
                }
            }
        }

        "terminal_input" => {
            if let MessagePayload::TerminalInput {
                ref session_id,
                ref data,
            } = msg.payload
            {
                sessions.terminal_input(session_id, data).await?;
            }
        }

        "terminal_resize" => {
            if let MessagePayload::TerminalResize {
                ref session_id,
                cols,
                rows,
            } = msg.payload
            {
                sessions.terminal_resize(session_id, cols, rows).await?;
            }
        }

        "terminal_kill" => {
            if let MessagePayload::TerminalKill { ref session_id } = msg.payload {
                sessions.terminal_kill(session_id).await?;
            }
        }

        "file_list" => {
            if let MessagePayload::FileList {
                ref file_id,
                ref path,
            } = msg.payload
            {
                if config.enable_file_browser {
                    let result = files::list_directory(&config.file_root, path);
                    let response = CdapMessage {
                        msg_type: "file_list_response".into(),
                        payload: MessagePayload::FileListResponse {
                            file_id: file_id.clone(),
                            entries: result.unwrap_or_default(),
                            error: None,
                        },
                    };
                    sender.lock().await.send_message(&response).await?;
                }
            }
        }

        "file_read" => {
            if let MessagePayload::FileRead {
                ref file_id,
                ref path,
                offset,
                length,
            } = msg.payload
            {
                if config.enable_file_browser {
                    let result = files::read_file_chunk(&config.file_root, path, offset, length);
                    let response = CdapMessage {
                        msg_type: "file_read_response".into(),
                        payload: MessagePayload::FileReadResponse {
                            file_id: file_id.clone(),
                            data: result.as_ref().ok().cloned().unwrap_or_default(),
                            eof: result.as_ref().map(|d| d.is_empty()).unwrap_or(true),
                            error: result.err().map(|e| e.to_string()),
                        },
                    };
                    sender.lock().await.send_message(&response).await?;
                }
            }
        }

        "file_write" => {
            if let MessagePayload::FileWrite {
                ref file_id,
                ref path,
                ref data,
                offset,
            } = msg.payload
            {
                if config.enable_file_browser {
                    let result = files::write_file_chunk(&config.file_root, path, data, offset);
                    let response = CdapMessage {
                        msg_type: "file_write_response".into(),
                        payload: MessagePayload::FileWriteResponse {
                            file_id: file_id.clone(),
                            success: result.is_ok(),
                            error: result.err().map(|e| e.to_string()),
                        },
                    };
                    sender.lock().await.send_message(&response).await?;
                }
            }
        }

        "file_delete" => {
            if let MessagePayload::FileDelete {
                ref file_id,
                ref path,
            } = msg.payload
            {
                if config.enable_file_browser {
                    let result = files::delete_path(&config.file_root, path);
                    let response = CdapMessage {
                        msg_type: "file_delete_response".into(),
                        payload: MessagePayload::FileDeleteResponse {
                            file_id: file_id.clone(),
                            success: result.is_ok(),
                            error: result.err().map(|e| e.to_string()),
                        },
                    };
                    sender.lock().await.send_message(&response).await?;
                }
            }
        }

        "clipboard_get" => {
            if config.enable_clipboard {
                let text = crate::clipboard::get_text().unwrap_or_default();
                let response = CdapMessage {
                    msg_type: "clipboard_update".into(),
                    payload: MessagePayload::ClipboardUpdate {
                        format: "text".into(),
                        data: text,
                    },
                };
                sender.lock().await.send_message(&response).await?;
            }
        }

        "clipboard_set" => {
            if let MessagePayload::ClipboardSet { ref data, .. } = msg.payload {
                if config.enable_clipboard {
                    let _ = crate::clipboard::set_text(data);
                }
            }
        }

        "screenshot_capture" => {
            let data = desktop::capture_screenshot_base64();
            let response = CdapMessage {
                msg_type: "screenshot_response".into(),
                payload: MessagePayload::ScreenshotResponse {
                    format: "jpeg".into(),
                    data: data.unwrap_or_default(),
                    width: 0,
                    height: 0,
                },
            };
            sender.lock().await.send_message(&response).await?;
        }

        "desktop_start" | "desktop_input" | "desktop_stop" => {
            // Remote desktop handled by desktop module
            desktop::handle_desktop_message(msg, sender, sessions).await?;
        }

        "ping" => {
            let response = CdapMessage {
                msg_type: "pong".into(),
                payload: MessagePayload::Generic(serde_json::json!({
                    "timestamp": chrono::Utc::now().timestamp_millis()
                })),
            };
            sender.lock().await.send_message(&response).await?;
        }

        other => {
            warn!("CDAP: Unhandled message type: {}", other);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
//  Command dispatch
// ---------------------------------------------------------------------------

async fn dispatch_command(
    widget_id: &str,
    action: &str,
    value: &serde_json::Value,
    _config: &CdapConfig,
    _sessions: &mut SessionManager,
) -> Result<serde_json::Value> {
    match widget_id {
        // System management buttons
        "btn_restart" if action == "trigger" => {
            sysmanage::restart_system()?;
            Ok(serde_json::json!({"status": "initiated"}))
        }
        "btn_shutdown" if action == "trigger" => {
            sysmanage::shutdown_system()?;
            Ok(serde_json::json!({"status": "initiated"}))
        }
        "btn_lock" if action == "trigger" => {
            sysmanage::lock_screen()?;
            Ok(serde_json::json!({"status": "ok"}))
        }
        "btn_logoff" if action == "trigger" => {
            sysmanage::logoff_user()?;
            Ok(serde_json::json!({"status": "ok"}))
        }
        "btn_flush_dns" if action == "trigger" => {
            let output = sysmanage::flush_dns()?;
            Ok(serde_json::json!({"output": output}))
        }
        "btn_clear_temp" if action == "trigger" => {
            let cleaned = sysmanage::clear_temp()?;
            Ok(serde_json::json!({"cleaned_mb": cleaned}))
        }

        // Process management
        "process_list" if action == "query" => {
            let procs = sysmanage::list_processes()?;
            Ok(serde_json::json!(procs))
        }
        "process_list" if action == "execute" => {
            let pid = value.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            sysmanage::kill_process(pid)?;
            Ok(serde_json::json!({"status": "killed", "pid": pid}))
        }

        // Service management
        "service_list" if action == "query" => {
            let services = sysmanage::list_services()?;
            Ok(serde_json::json!(services))
        }
        "service_list" if action == "execute" => {
            let name = value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let cmd = value
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("restart");
            sysmanage::control_service(name, cmd)?;
            Ok(serde_json::json!({"status": "ok", "service": name, "command": cmd}))
        }

        // Network management
        "network_info" if action == "query" => {
            let info = network_mgmt::get_network_info()?;
            Ok(serde_json::json!(info))
        }
        "network_info" if action == "execute" => {
            let cmd = value
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let result = network_mgmt::execute_network_command(cmd, value)?;
            Ok(result)
        }

        // Firewall management
        "firewall_rules" if action == "query" => {
            let rules = security::list_firewall_rules()?;
            Ok(serde_json::json!(rules))
        }
        "firewall_rules" if action == "execute" => {
            let cmd = value
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            security::manage_firewall_rule(cmd, value)?;
            Ok(serde_json::json!({"status": "ok"}))
        }

        // Event log
        "event_log" if action == "query" => {
            let logs = security::get_event_log(
                value
                    .get("log_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("System"),
                value.get("count").and_then(|v| v.as_u64()).unwrap_or(50) as u32,
            )?;
            Ok(serde_json::json!(logs))
        }

        // Windows Defender
        "defender_status" if action == "query" => {
            let status = security::get_defender_status()?;
            Ok(serde_json::json!(status))
        }
        "defender_status" if action == "execute" => {
            let scan_type = value
                .get("scan_type")
                .and_then(|v| v.as_str())
                .unwrap_or("quick");
            security::start_defender_scan(scan_type)?;
            Ok(serde_json::json!({"status": "scan_started", "type": scan_type}))
        }

        // Automation
        "automation" if action == "execute" => {
            let script = value
                .get("script")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let engine = value
                .get("engine")
                .and_then(|v| v.as_str())
                .unwrap_or("powershell");
            let result = automation::execute_script(engine, script)?;
            Ok(serde_json::json!(result))
        }
        "scheduled_tasks" if action == "query" => {
            let tasks = automation::list_scheduled_tasks()?;
            Ok(serde_json::json!(tasks))
        }
        "scheduled_tasks" if action == "execute" => {
            let task_name = value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let cmd = value
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("run");
            automation::manage_scheduled_task(task_name, cmd)?;
            Ok(serde_json::json!({"status": "ok", "task": task_name}))
        }

        // Installed software
        "installed_software" if action == "query" => {
            let software = sysmanage::list_installed_software()?;
            Ok(serde_json::json!(software))
        }

        // Startup programs
        "startup_programs" if action == "query" => {
            let programs = sysmanage::list_startup_programs()?;
            Ok(serde_json::json!(programs))
        }

        // User accounts
        "user_accounts" if action == "query" => {
            let users = security::list_user_accounts()?;
            Ok(serde_json::json!(users))
        }

        // Environment variables
        "env_vars" if action == "query" => {
            let vars = sysmanage::get_environment_variables()?;
            Ok(serde_json::json!(vars))
        }

        _ => {
            anyhow::bail!("Unknown widget/action: {}/{}", widget_id, action)
        }
    }
}

// ---------------------------------------------------------------------------
//  Session manager
// ---------------------------------------------------------------------------

/// Manages active terminal, desktop, and media sessions.
struct SessionManager {
    terminals: std::collections::HashMap<String, terminal::TerminalSession>,
    desktop_session: Option<desktop::DesktopSession>,
}

impl SessionManager {
    fn new() -> Self {
        Self {
            terminals: std::collections::HashMap::new(),
            desktop_session: None,
        }
    }

    fn list_active(&self) -> Vec<String> {
        let mut list: Vec<String> = self
            .terminals
            .keys()
            .map(|k| format!("terminal:{}", k))
            .collect();
        if self.desktop_session.is_some() {
            list.push("desktop".into());
        }
        list
    }

    async fn start_terminal(
        &mut self,
        session_id: String,
        shell: String,
        cols: u16,
        rows: u16,
        sender: Arc<Mutex<CdapSender>>,
    ) -> Result<()> {
        let session = terminal::TerminalSession::start(
            session_id.clone(),
            &shell,
            cols,
            rows,
            sender,
        )
        .await?;
        self.terminals.insert(session_id, session);
        Ok(())
    }

    async fn terminal_input(&mut self, session_id: &str, data: &str) -> Result<()> {
        if let Some(session) = self.terminals.get_mut(session_id) {
            session.write_input(data).await?;
        }
        Ok(())
    }

    async fn terminal_resize(&mut self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        if let Some(session) = self.terminals.get_mut(session_id) {
            session.resize(cols, rows)?;
        }
        Ok(())
    }

    async fn terminal_kill(&mut self, session_id: &str) -> Result<()> {
        if let Some(mut session) = self.terminals.remove(session_id) {
            session.kill().await?;
        }
        Ok(())
    }

    async fn close_all(&mut self) {
        for (_, mut session) in self.terminals.drain() {
            let _ = session.kill().await;
        }
        if let Some(session) = self.desktop_session.take() {
            session.stop().await;
        }
    }
}

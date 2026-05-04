//! Tauri IPC commands — bridge between SolidJS frontend and Rust backend.
//!
//! Commands transparently dispatch between Yomie native protocol (HTTP +
//! WebSocket relay) and legacy RustDesk-compatible protocol (UDP signal + TCP
//! relay) based on the `native_protocol` setting.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{Emitter, State};

use crate::cdap::{CdapAgent, CdapConfig, CdapStatus};
use crate::config::Settings;
use crate::discovery::{DiscoveredServer, LanDiscoveryService, LanDiscoveryStatus};
use crate::identity;
use crate::network::bd_registration::{BdRegistrationService, BdRegistrationStatus};
use crate::network::bd_relay::BdRelayConnection;
use crate::network::registration::{RegistrationService, RegistrationStatus};
use crate::network::session::Session;
use crate::inventory::InventoryCollector;
use crate::inventory::collector::InventoryStatus;
use crate::chat::ChatService;
use crate::remote::RemoteAgent;
use crate::remote::{SessionManager, SessionCommand};

/// A single activity log entry.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityEntry {
    pub action: String,
    pub target: String,
    pub timestamp: String,
    pub details: String,
}

/// MGMT-C3: central flag for TLS hardening. Defaults to allow self-signed certs
/// (preserves backwards compatibility with existing Yomie deployments).
/// Set `BETTERDESK_STRICT_TLS=1` to enforce strict certificate validation.
fn strict_tls_enabled() -> bool {
    matches!(
        std::env::var("BETTERDESK_STRICT_TLS").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    )
}

fn warn_self_signed_once() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::SeqCst) {
        eprintln!(
            "[Yomie] WARNING: TLS certificate validation is DISABLED for \
             server probing/login. This is insecure against MITM. Set \
             BETTERDESK_STRICT_TLS=1 once the server has a proper certificate."
        );
    }
}

/// Build a reqwest client honouring the BETTERDESK_STRICT_TLS gate.
fn build_http_client(timeout_secs: u64) -> Result<reqwest::Client, reqwest::Error> {
    let mut b = reqwest::Client::builder().timeout(std::time::Duration::from_secs(timeout_secs));
    if !strict_tls_enabled() {
        warn_self_signed_once();
        b = b.danger_accept_invalid_certs(true);
    }
    b.build()
}

/// Simple in-memory activity log tracker.
pub struct ActivityTracker {
    entries: Vec<ActivityEntry>,
    max_entries: usize,
}

impl ActivityTracker {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_entries: 500,
        }
    }

    pub fn log(&mut self, action: &str, target: &str, details: &str) {
        let entry = ActivityEntry {
            action: action.to_string(),
            target: target.to_string(),
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            details: details.to_string(),
        };
        self.entries.push(entry);
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }

    pub fn get_recent(&self, limit: usize) -> Vec<ActivityEntry> {
        let start = if self.entries.len() > limit {
            self.entries.len() - limit
        } else {
            0
        };
        self.entries[start..].iter().rev().cloned().collect()
    }
}

/// Application state shared across Tauri commands.
pub struct AppState {
    pub settings: Mutex<Settings>,
    pub session: Mutex<Option<Session>>,
    pub registration: Mutex<Option<RegistrationService>>,
    /// Yomie native HTTP registration service.
    pub bd_registration: Mutex<Option<BdRegistrationService>>,
    /// Yomie native WebSocket relay connection.
    pub bd_relay: Mutex<Option<BdRelayConnection>>,
    /// Inventory collector service.
    pub inventory: Mutex<Option<InventoryCollector>>,
    /// LAN discovery scanner service.
    pub discovery: Mutex<Option<LanDiscoveryService>>,
    /// Instant chat WebSocket service.
    pub chat_agent: Mutex<Option<ChatService>>,
    /// Remote desktop standby agent.
    pub remote_agent: Mutex<Option<RemoteAgent>>,
    /// CDAP desktop agent — system management, automation, telemetry.
    pub cdap_agent: Mutex<Option<CdapAgent>>,
    /// In-memory activity tracker.
    pub activity: Mutex<ActivityTracker>,
    /// Active relay session manager for remote desktop (Phase 43 fix).
    pub session_manager: Mutex<Option<SessionManager>>,
    /// Notification state: IDs that have been read.
    pub read_notifs: Mutex<std::collections::HashSet<String>>,
    /// Notification state: IDs that have been dismissed.
    pub dismissed_notifs: Mutex<std::collections::HashSet<String>>,
    /// Shared HTTP client with cookie store for session-based auth.
    /// Used by `api_proxy` to forward WebView requests through Rust,
    /// bypassing CORS and mixed-content restrictions.
    pub http_client: reqwest::Client,
}

/// Serializable connection status for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionStatus {
    pub state: String,
    pub peer_id: Option<String>,
    pub peer_info: Option<serde_json::Value>,
    pub latency_ms: Option<u32>,
    pub error: Option<String>,
}

/// Unified registration status — wraps both BD native and legacy structs.
#[derive(Debug, Clone, Serialize)]
pub struct UnifiedRegistrationStatus {
    pub registered: bool,
    pub device_id: String,
    pub server_address: String,
    pub heartbeat_count: u64,
    pub last_error: Option<String>,
    /// true when using Yomie native protocol.
    pub native_protocol: bool,
    /// BD-native only: WebSocket signal channel connected.
    pub signal_connected: Option<bool>,
    /// BD-native only: pending incoming connection requests.
    pub pending_connections: Option<Vec<crate::network::bd_registration::PendingConnection>>,
    /// BD-native only: current enrollment phase.
    pub enrollment_phase: Option<String>,
    /// BD-native only: operator-assigned sync mode.
    pub sync_mode: Option<String>,
    /// BD-native only: branding config from server.
    pub branding: Option<crate::network::bd_registration::BrandingConfig>,
    /// BD-native only: operator-assigned display name.
    pub display_name: Option<String>,
}

impl From<RegistrationStatus> for UnifiedRegistrationStatus {
    fn from(s: RegistrationStatus) -> Self {
        Self {
            registered: s.registered,
            device_id: s.device_id,
            server_address: s.server_address,
            heartbeat_count: s.heartbeat_count,
            last_error: s.last_error,
            native_protocol: false,
            signal_connected: None,
            pending_connections: None,
            enrollment_phase: None,
            sync_mode: None,
            branding: None,
            display_name: None,
        }
    }
}

impl From<BdRegistrationStatus> for UnifiedRegistrationStatus {
    fn from(s: BdRegistrationStatus) -> Self {
        let phase_str = format!("{:?}", s.phase);
        Self {
            registered: s.registered,
            device_id: s.device_id,
            server_address: s.server_url,
            heartbeat_count: s.heartbeat_count,
            last_error: s.last_error,
            native_protocol: true,
            signal_connected: Some(s.signal_connected),
            pending_connections: Some(s.pending_connections),
            enrollment_phase: Some(phase_str),
            sync_mode: s.sync_mode,
            branding: s.branding,
            display_name: s.display_name,
        }
    }
}

// ---- Configuration ----

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Result<Settings, String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    Ok(settings.clone())
}

#[tauri::command]
pub fn save_config(state: State<'_, AppState>, config: Settings) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;

    // Server configuration changes require admin privileges
    let server_changed = config.server_address != settings.server_address
        || config.console_url != settings.console_url
        || config.server_key != settings.server_key
        || config.native_protocol != settings.native_protocol;

    if server_changed && !crate::service::is_elevated() {
        return Err("Administrator privileges required to change server settings".into());
    }

    *settings = config.clone();
    config.save().map_err(|e| e.to_string())
}

// ---- Identity ----

#[tauri::command]
pub fn get_device_id() -> Result<String, String> {
    identity::get_or_create_device_id().map_err(|e| e.to_string())
}

// ---- Connection ----

#[tauri::command]
pub async fn connect_to_peer(
    state: State<'_, AppState>,
    peer_id: String,
    server_url: Option<String>,
) -> Result<ConnectionStatus, String> {
    // Security: validate peer_id format to prevent injection
    if peer_id.is_empty() || peer_id.len() > 64
        || !peer_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err("Invalid peer_id format".to_string());
    }

    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    // Use the frontend-provided server URL (from localStorage) so the request
    // goes to the same origin where session cookies were established.
    // Fall back to settings.bd_api_url() if not provided.
    let base_url = server_url
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| settings.bd_api_url());

    log::info!("[connect_to_peer] peer={} base_url={} native_protocol={} has_token={} console_url={}",
        peer_id, base_url, settings.native_protocol,
        settings.access_token.as_ref().map_or(false, |t| !t.is_empty()),
        settings.console_url);

    // Try BD native protocol first if configured.
    // Token may come from operator_login (stored in keyring) or session cookies
    // are used as fallback via the shared http_client.
    if settings.native_protocol {
        let my_id = identity::get_or_create_device_id().map_err(|e| e.to_string())?;
        let client = &state.http_client;

        // Pre-flight: check if session cookie is valid by hitting /api/auth/verify
        {
            let verify_url = format!("{}/api/auth/verify", base_url);
            match client.get(&verify_url)
                .header("Origin", "https://tauri.localhost")
                .send()
                .await
            {
                Ok(resp) => {
                    let st = resp.status().as_u16();
                    let body = resp.text().await.unwrap_or_default();
                    log::info!("[connect_to_peer] cookie-jar session check: status={} body_prefix={}", st, &body[..body.len().min(100)]);
                },
                Err(e) => {
                    log::warn!("[connect_to_peer] cookie-jar session check failed: {}", e);
                }
            }
        }

        let relay = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            BdRelayConnection::connect(&base_url, &settings, client, &my_id, &peer_id),
        )
        .await
        .map_err(|_| "Connection timed out (30s)".to_string())?
        .map_err(|e| e.to_string())?;

        let relay_status = relay.status();
        let status = ConnectionStatus {
            state: format!("{:?}", relay_status.state).to_lowercase(),
            peer_id: Some(relay_status.peer_id),
            peer_info: None,
            latency_ms: None,
            error: relay_status.last_error,
        };

        let mut bd_lock = state.bd_relay.lock().map_err(|e| e.to_string())?;
        *bd_lock = Some(relay);
        Ok(status)
    } else {
        // Legacy RustDesk-compatible protocol
        let session = Session::new(&settings, &peer_id).map_err(|e| e.to_string())?;
        let status = session.status();
        let mut session_lock = state.session.lock().map_err(|e| e.to_string())?;
        *session_lock = Some(session);
        Ok(status)
    }
}

#[tauri::command]
pub fn disconnect(state: State<'_, AppState>) -> Result<(), String> {
    // Stop SessionManager first (drops relay channels gracefully)
    {
        let mut mgr_lock = state.session_manager.lock().map_err(|e| e.to_string())?;
        if let Some(mgr) = mgr_lock.take() {
            mgr.stop();
        }
    }
    // Disconnect BD native relay if active
    {
        let mut bd_lock = state.bd_relay.lock().map_err(|e| e.to_string())?;
        if let Some(relay) = bd_lock.take() {
            relay.close();
        }
    }
    // Disconnect legacy session if active
    {
        let mut session_lock = state.session.lock().map_err(|e| e.to_string())?;
        if let Some(session) = session_lock.take() {
            session.disconnect();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn authenticate(
    state: State<'_, AppState>,
    password: String,
) -> Result<ConnectionStatus, String> {
    // Extract Arc-based Session from Mutex immediately to avoid holding guard across await
    let session = {
        let lock = state.session.lock().map_err(|e| e.to_string())?;
        lock.as_ref().ok_or("No active session")?.clone_handle()
    };
    session
        .authenticate(&password)
        .await
        .map_err(|e| e.to_string())?;
    Ok(session.status())
}

// ---- Input ----

#[derive(Debug, Deserialize)]
pub struct KeyEventPayload {
    pub key: String,
    pub down: bool,
    pub modifiers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct MouseEventPayload {
    pub x: i32,
    pub y: i32,
    pub mask: u32,
    pub modifiers: Vec<String>,
}

#[tauri::command]
pub fn send_key_event(
    state: State<'_, AppState>,
    event: KeyEventPayload,
) -> Result<(), String> {
    let session_lock = state.session.lock().map_err(|e| e.to_string())?;
    let session = session_lock.as_ref().ok_or("No active session")?;
    session.send_key(&event).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn send_mouse_event(
    state: State<'_, AppState>,
    event: MouseEventPayload,
) -> Result<(), String> {
    let session_lock = state.session.lock().map_err(|e| e.to_string())?;
    let session = session_lock.as_ref().ok_or("No active session")?;
    session.send_mouse(&event).map_err(|e| e.to_string())
}

/// Simulate local input via enigo (used when this machine is the target).
#[tauri::command]
pub fn simulate_local_key(event: KeyEventPayload) -> Result<(), String> {
    crate::input::simulate_key(&event.key, event.down, &event.modifiers)
        .map_err(|e| e.to_string())
}

/// Simulate local mouse input via enigo.
#[tauri::command]
pub fn simulate_local_mouse(event: MouseEventPayload) -> Result<(), String> {
    crate::input::simulate_mouse(event.x, event.y, event.mask, &event.modifiers)
        .map_err(|e| e.to_string())
}

/// Simulate typing text via enigo.
#[tauri::command]
pub fn simulate_local_text(text: String) -> Result<(), String> {
    crate::input::simulate_text(&text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_connection_state(state: State<'_, AppState>) -> Result<ConnectionStatus, String> {
    // Check BD native relay first
    {
        let bd_lock = state.bd_relay.lock().map_err(|e| e.to_string())?;
        if let Some(relay) = bd_lock.as_ref() {
            let rs = relay.status();
            return Ok(ConnectionStatus {
                state: format!("{:?}", rs.state).to_lowercase(),
                peer_id: Some(rs.peer_id),
                peer_info: None,
                latency_ms: None,
                error: rs.last_error,
            });
        }
    }
    // Fall back to legacy session
    let session_lock = state.session.lock().map_err(|e| e.to_string())?;
    match session_lock.as_ref() {
        Some(session) => Ok(session.status()),
        None => Ok(ConnectionStatus {
            state: "idle".into(),
            peer_id: None,
            peer_info: None,
            latency_ms: None,
            error: None,
        }),
    }
}

// ---- Registration ----

#[tauri::command]
pub fn start_registration(state: State<'_, AppState>) -> Result<UnifiedRegistrationStatus, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let has_server = !settings.server_address.is_empty()
        || !settings.console_url.is_empty();
    if !has_server {
        return Err("Server address not configured".into());
    }

    let device_id = identity::get_or_create_device_id().map_err(|e| e.to_string())?;

    if settings.native_protocol {
        // Stop legacy if running
        {
            let mut old_lock = state.registration.lock().map_err(|e| e.to_string())?;
            if let Some(old) = old_lock.take() {
                old.stop();
            }
        }

        let mut bd_lock = state.bd_registration.lock().map_err(|e| e.to_string())?;
        if let Some(old) = bd_lock.take() {
            old.stop();
        }

        let service = BdRegistrationService::start(&settings, &device_id);
        let status: UnifiedRegistrationStatus = service.status().into();
        *bd_lock = Some(service);
        Ok(status)
    } else {
        // Stop BD native if running
        {
            let mut bd_lock = state.bd_registration.lock().map_err(|e| e.to_string())?;
            if let Some(old) = bd_lock.take() {
                old.stop();
            }
        }

        let mut reg_lock = state.registration.lock().map_err(|e| e.to_string())?;
        if let Some(old) = reg_lock.take() {
            old.stop();
        }

        let service = RegistrationService::start(&settings, &device_id);
        let status: UnifiedRegistrationStatus = service.status().into();
        *reg_lock = Some(service);
        Ok(status)
    }
}

#[tauri::command]
pub fn stop_registration(state: State<'_, AppState>) -> Result<(), String> {
    // Stop whichever is active
    {
        let mut reg_lock = state.registration.lock().map_err(|e| e.to_string())?;
        if let Some(service) = reg_lock.take() {
            service.stop();
        }
    }
    {
        let mut bd_lock = state.bd_registration.lock().map_err(|e| e.to_string())?;
        if let Some(service) = bd_lock.take() {
            service.stop();
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_registration_status(state: State<'_, AppState>) -> Result<UnifiedRegistrationStatus, String> {
    // Check BD native first
    {
        let bd_lock = state.bd_registration.lock().map_err(|e| e.to_string())?;
        if let Some(service) = bd_lock.as_ref() {
            return Ok(service.status().into());
        }
    }
    // Fall back to legacy
    {
        let reg_lock = state.registration.lock().map_err(|e| e.to_string())?;
        if let Some(service) = reg_lock.as_ref() {
            return Ok(service.status().into());
        }
    }
    // Neither running
    let device_id = identity::get_or_create_device_id().unwrap_or_default();
    Ok(UnifiedRegistrationStatus {
        registered: false,
        device_id,
        server_address: String::new(),
        heartbeat_count: 0,
        last_error: Some("Registration not started".into()),
        native_protocol: false,
        signal_connected: None,
        pending_connections: None,
        enrollment_phase: None,
        sync_mode: None,
        branding: None,
        display_name: None,
    })
}

// ---- Inventory ----

#[tauri::command]
pub fn get_inventory_status(state: State<'_, AppState>) -> Result<InventoryStatus, String> {
    let inv_lock = state.inventory.lock().map_err(|e| e.to_string())?;
    match inv_lock.as_ref() {
        Some(collector) => Ok(collector.status()),
        None => Ok(InventoryStatus {
            running: false,
            phase: crate::inventory::collector::SyncPhase::Idle,
            last_upload_at: None,
            upload_count: 0,
            last_error: Some("Inventory collector not running".into()),
            hardware: None,
            history_size: 0,
            sync_mode: "standard".into(),
        }),
    }
}

#[tauri::command]
pub async fn collect_inventory_now(
    _state: State<'_, AppState>,
) -> Result<crate::inventory::HardwareInfo, String> {
    // Collect hardware info on a blocking thread
    let hw = tokio::task::spawn_blocking(crate::inventory::hardware::collect)
        .await
        .map_err(|e| e.to_string())?;
    Ok(hw)
}

// ---- LAN Discovery ----

#[tauri::command]
pub fn start_lan_discovery(state: State<'_, AppState>) -> Result<LanDiscoveryStatus, String> {
    let mut disc_lock = state.discovery.lock().map_err(|e| e.to_string())?;
    // Stop any existing scan
    if let Some(old) = disc_lock.take() {
        old.stop();
    }
    let service = LanDiscoveryService::start();
    let status = service.status();
    *disc_lock = Some(service);
    Ok(status)
}

#[tauri::command]
pub fn stop_lan_discovery(state: State<'_, AppState>) -> Result<(), String> {
    let mut disc_lock = state.discovery.lock().map_err(|e| e.to_string())?;
    if let Some(service) = disc_lock.take() {
        service.stop();
    }
    Ok(())
}

#[tauri::command]
pub fn get_discovered_servers(state: State<'_, AppState>) -> Result<Vec<DiscoveredServer>, String> {
    let disc_lock = state.discovery.lock().map_err(|e| e.to_string())?;
    match disc_lock.as_ref() {
        Some(service) => Ok(service.discovered_servers()),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub fn get_discovery_status(state: State<'_, AppState>) -> Result<LanDiscoveryStatus, String> {
    let disc_lock = state.discovery.lock().map_err(|e| e.to_string())?;
    match disc_lock.as_ref() {
        Some(service) => Ok(service.status()),
        None => Ok(LanDiscoveryStatus {
            scanning: false,
            servers: Vec::new(),
            scans_completed: 0,
            last_error: Some("Discovery not started".into()),
        }),
    }
}

/// Request registration with a discovered server.
///
/// Sends an HTTP POST to the server's `/api/bd/register-request` endpoint
/// with this device's identity information.

/// Browse for Yomie servers via mDNS/DNS-SD.
/// Returns servers advertising `_yomie._tcp` on the local network.
#[tauri::command]
pub async fn discover_mdns_servers() -> Result<Vec<crate::discovery::MdnsServer>, String> {
    let servers = tokio::task::spawn_blocking(|| {
        crate::discovery::browse_mdns_servers(std::time::Duration::from_secs(10))
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(servers)
}

#[tauri::command]
pub async fn request_server_registration(
    console_url: String,
) -> Result<serde_json::Value, String> {
    let device_id = identity::get_or_create_device_id().map_err(|e| e.to_string())?;
    let hostname = whoami::devicename();
    let platform = std::env::consts::OS.to_string();
    let version = env!("CARGO_PKG_VERSION").to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "device_id": device_id,
        "hostname": hostname,
        "platform": platform,
        "version": version,
    });

    let url = format!("{}/api/bd/register-request", console_url.trim_end_matches('/'));

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to send registration request: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Registration request failed (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).map_err(|e| format!("Invalid response: {}", e))
}

/// Poll the server for registration approval status.
///
/// Returns { success, status, config? } — when status is "approved",
/// config contains the server credentials to auto-configure the client.
#[tauri::command]
pub async fn poll_registration_status(
    console_url: String,
) -> Result<serde_json::Value, String> {
    let device_id = identity::get_or_create_device_id().map_err(|e| e.to_string())?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!(
        "{}/api/bd/register-status?device_id={}",
        console_url.trim_end_matches('/'),
        urlencoding::encode(&device_id)
    );

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to poll status: {}", e))?;

    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("Invalid response: {}", e))
}

/// Apply server configuration received after registration approval.
///
/// Updates settings with the server's console_url, server_address, server_key,
/// and access_token, then saves to disk.
#[tauri::command]
pub fn apply_server_config(
    state: State<'_, AppState>,
    console_url: String,
    server_address: String,
    server_key: String,
    access_token: String,
) -> Result<(), String> {
    if !crate::service::is_elevated() {
        return Err("Administrator privileges required to change server configuration".into());
    }

    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.console_url = console_url;
    settings.server_address = server_address;
    settings.server_key = server_key;
    settings.access_token = if access_token.is_empty() {
        None
    } else {
        Some(access_token)
    };
    settings.native_protocol = true;
    settings.save().map_err(|e| e.to_string())
}

/// Test connection to a Yomie server without saving config.
///
/// Attempts to reach the server's HTTP API and signal port to verify
/// connectivity before the user commits to saving the configuration.
#[tauri::command]
pub async fn test_server_connection(
    server_address: String,
    console_url: String,
) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())?;

    // Derive API URL (Go server on signal_port - 2, typically 21114)
    let api_url = if !console_url.is_empty() {
        let url = console_url.trim_end_matches('/');
        if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else {
            format!("http://{}", url)
        }
    } else {
        let host = if let Some(colon) = server_address.rfind(':') {
            &server_address[..colon]
        } else {
            &server_address
        };
        format!("http://{}:5000", host)
    };

    // Try /api/server/stats (public, no auth required on Go server)
    let stats_url = format!("{}/api/server/stats", api_url);
    match client.get(&stats_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                let server_key = body
                    .get("public_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                return Ok(serde_json::json!({
                    "success": true,
                    "server_key": server_key,
                    "server_version": body.get("version").and_then(|v| v.as_str()).unwrap_or(""),
                }));
            }
            // HTTP reachable but unexpected status — still partially valid
            Ok(serde_json::json!({
                "success": true,
                "server_key": "",
                "warning": format!("Server responded with HTTP {}", resp.status()),
            }))
        }
        Err(e) => {
            // Console might not be reachable, try Go API port directly
            let host = if let Some(colon) = server_address.rfind(':') {
                &server_address[..colon]
            } else {
                &server_address
            };
            let go_api_url = format!("http://{}:21114/api/server/stats", host);

            match client.get(&go_api_url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let body: serde_json::Value = resp.json().await.unwrap_or_default();
                    let server_key = body
                        .get("public_key")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Ok(serde_json::json!({
                        "success": true,
                        "server_key": server_key,
                    }))
                }
                _ => {
                    Ok(serde_json::json!({
                        "success": false,
                        "error": format!("Cannot reach server at {} or {}: {}", api_url, go_api_url, e),
                    }))
                }
            }
        }
    }
}

/// Auto-connect to a Yomie server using only the IP/hostname.
///
/// Performs full endpoint discovery, connection testing, key retrieval,
/// config save, and registration — all in one step.
/// Returns progress updates as a final result object.
#[tauri::command]
pub async fn auto_connect_server(
    state: State<'_, AppState>,
    address: String,
) -> Result<serde_json::Value, String> {
    if !crate::service::is_elevated() {
        return Err("Administrator privileges required to change server configuration".into());
    }

    let host = address.trim().trim_end_matches('/');
    if host.is_empty() {
        return Err("Server address is required".into());
    }

    // Strip protocol prefix if user pasted a URL
    let host = host
        .strip_prefix("https://")
        .or_else(|| host.strip_prefix("http://"))
        .unwrap_or(host);

    // Strip any port suffix — we'll probe standard ports
    let host = if let Some(colon) = host.rfind(':') {
        if host[colon + 1..].chars().all(|c| c.is_ascii_digit()) {
            &host[..colon]
        } else {
            host
        }
    } else {
        host
    };

    let client = build_http_client(6).map_err(|e| e.to_string())?;

    let mut steps: Vec<serde_json::Value> = Vec::new();
    let mut server_key = String::new();
    let console_url;
    let mut api_reachable = false;

    // Step 1: Probe Go API (port 21114)
    let go_api_url = format!("http://{}:21114/api/server/stats", host);
    match client.get(&go_api_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            server_key = body
                .get("public_key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            api_reachable = true;
            steps.push(serde_json::json!({
                "step": "api",
                "status": "ok",
                "detail": format!("Go API reachable on port 21114"),
                "version": body.get("version").and_then(|v| v.as_str()).unwrap_or(""),
            }));
        }
        Ok(resp) => {
            // Reachable but unexpected status — still OK
            api_reachable = true;
            steps.push(serde_json::json!({
                "step": "api",
                "status": "ok",
                "detail": format!("Go API responded with HTTP {}", resp.status()),
            }));
        }
        Err(e) => {
            steps.push(serde_json::json!({
                "step": "api",
                "status": "warn",
                "detail": format!("Go API not reachable on 21114: {}", e),
            }));
        }
    }

    // Step 2: Probe console (port 5000)
    let console_test_url = format!("http://{}:5000", host);
    match client.get(&console_test_url).send().await {
        Ok(resp) if resp.status().is_success() || resp.status().is_redirection() => {
            console_url = format!("http://{}:5000", host);
            steps.push(serde_json::json!({
                "step": "console",
                "status": "ok",
                "detail": "Web console reachable on port 5000",
            }));
        }
        _ => {
            // Console not available — non-blocking, just informational
            console_url = format!("http://{}:5000", host);
            steps.push(serde_json::json!({
                "step": "console",
                "status": "skip",
                "detail": "Web console not detected on port 5000 (optional)",
            }));
        }
    }

    // Step 3: Probe signal port (TCP 21116)
    let signal_addr = format!("{}:21116", host);
    let signal_ok = match tokio::time::timeout(
        std::time::Duration::from_secs(4),
        tokio::net::TcpStream::connect(&signal_addr),
    )
    .await
    {
        Ok(Ok(_stream)) => {
            steps.push(serde_json::json!({
                "step": "signal",
                "status": "ok",
                "detail": "Signal server reachable on port 21116",
            }));
            true
        }
        _ => {
            steps.push(serde_json::json!({
                "step": "signal",
                "status": "error",
                "detail": "Cannot reach signal server on port 21116",
            }));
            false
        }
    };

    // If neither API nor signal is reachable, fail
    if !api_reachable && !signal_ok {
        return Ok(serde_json::json!({
            "success": false,
            "error": format!("Cannot reach server at {}. Check the address and ensure the server is running.", host),
            "steps": steps,
        }));
    }

    // Step 4: Save configuration (disk I/O — run on blocking thread)
    let server_address = format!("{}:21116", host);
    let settings_to_save = {
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.console_url = console_url.clone();
        settings.server_address = server_address.clone();
        settings.server_key = server_key.clone();
        settings.access_token = None;
        settings.native_protocol = true;
        settings.clone()
        // MutexGuard dropped here — before any .await
    };
    tokio::task::spawn_blocking(move || settings_to_save.save())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    steps.push(serde_json::json!({
        "step": "config",
        "status": "ok",
        "detail": "Configuration saved",
    }));

    // Step 5: Start registration (reuses same logic as start_registration)
    let device_id = crate::identity::get_or_create_device_id().unwrap_or_default();
    {
        // Stop any existing registration services
        if let Ok(mut old) = state.bd_registration.lock() {
            if let Some(svc) = old.take() {
                svc.stop();
            }
        }
        if let Ok(mut old) = state.registration.lock() {
            if let Some(svc) = old.take() {
                svc.stop();
            }
        }

        let settings = state.settings.lock().map_err(|e| e.to_string())?.clone();
        if settings.native_protocol {
            // Yomie native: HTTP enrollment + CDAP + management
            let svc = BdRegistrationService::start(&settings, &device_id);
            if let Ok(mut lock) = state.bd_registration.lock() {
                *lock = Some(svc);
            }
            // Also start signal keepalive — UDP RegisterPeer heartbeats keep
            // the device visible in the signal server's in-memory peer map.
            // Without this, the web remote client sees the device as "offline"
            // because the signal server only tracks UDP/TCP-registered peers.
            let sig_svc = RegistrationService::start(&settings, &device_id);
            if let Ok(mut lock) = state.registration.lock() {
                *lock = Some(sig_svc);
            }
        } else {
            let svc = RegistrationService::start(&settings, &device_id);
            if let Ok(mut lock) = state.registration.lock() {
                *lock = Some(svc);
            }
        }
    }
    steps.push(serde_json::json!({
        "step": "register",
        "status": "ok",
        "detail": format!("Registration started (device: {})", device_id),
        "device_id": device_id,
    }));

    // CDAP auto-configure removed — desktop clients now use the management
    // WebSocket on the Go API port (21114) instead of CDAP gateway (21122).
    // The management WS is started automatically by BdRegistrationService.

    Ok(serde_json::json!({
        "success": true,
        "server_address": server_address,
        "console_url": console_url,
        "server_key": server_key,
        "steps": steps,
    }))
}

// ---- Admin gate ----

/// Returns true when the current process has admin/root elevation.
/// The frontend uses this to conditionally show admin-only UI.
#[tauri::command]
pub fn is_admin() -> bool {
    crate::service::is_elevated()
}

/// Apply a configuration change via an elevated helper process.
///
/// Spawns the same binary with `--apply-config <base64-json>` using UAC
/// elevation (Windows) or pkexec (Linux). The elevated process writes the
/// config file and exits. After success, the caller should reload settings.
#[tauri::command]
pub async fn elevate_and_apply_config(
    state: State<'_, AppState>,
    config_json: String,
) -> Result<(), String> {
    // If already elevated, write directly
    if crate::service::is_elevated() {
        let settings: Settings = serde_json::from_str(&config_json)
            .map_err(|e| e.to_string())?;
        let settings_cl = settings.clone();
        tokio::task::spawn_blocking(move || settings_cl.save())
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        // Update in-memory state
        let mut lock = state.settings.lock().map_err(|e| e.to_string())?;
        *lock = settings;
        return Ok(());
    }

    // Spawn elevated helper (blocks up to ~2s waiting for the process)
    tokio::task::spawn_blocking(move || {
        crate::service::apply_config_elevated(&config_json)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // Reload settings from disk (written by the elevated process)
    let fresh = Settings::load().unwrap_or_default();
    let mut lock = state.settings.lock().map_err(|e| e.to_string())?;
    *lock = fresh;

    Ok(())
}

/// Restart the application with elevated (admin) privileges.
/// The current process exits after launching the elevated instance.
#[tauri::command]
pub fn elevate_restart() -> Result<(), String> {
    crate::service::request_elevation().map_err(|e| e.to_string())
}

// ---- Organization Login ----

/// Login to an organization account on the Yomie server.
/// Returns user info + JWT token on success.
#[tauri::command]
pub async fn org_login(
    server_url: String,
    org_slug: String,
    username: String,
    password: String,
) -> Result<serde_json::Value, String> {
    let client = build_http_client(10).map_err(|e| e.to_string())?;

    let url = format!("{}/api/org/login", server_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "org_slug": org_slug,
        "username": username,
        "password": password,
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Connection failed: {}", e))?;

    let status = resp.status();
    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid response: {}", e))?;

    if !status.is_success() {
        let err_msg = data.get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("Login failed");
        return Err(err_msg.to_string());
    }

    Ok(data)
}

/// Detect the system locale for auto-language selection.
#[tauri::command]
pub fn get_system_locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en".into())
}

// ---- Chat ----

#[tauri::command]
pub fn get_chat_status(state: State<'_, AppState>) -> Result<crate::chat::ChatStatus, String> {
    let lock = state.chat_agent.lock().map_err(|e| e.to_string())?;
    match lock.as_ref() {
        Some(agent) => Ok(agent.status()),
        None => Ok(crate::chat::ChatStatus {
            connected: false,
            unread_count: 0,
            messages: Vec::new(),
            contacts: Vec::new(),
            groups: Vec::new(),
        }),
    }
}

#[tauri::command]
pub fn send_chat_message(
    state: State<'_, AppState>,
    text: String,
    conversation_id: Option<String>,
) -> Result<(), String> {
    let lock = state.chat_agent.lock().map_err(|e| e.to_string())?;
    match lock.as_ref() {
        Some(agent) => {
            agent.send(text, conversation_id);
            Ok(())
        }
        None => Err("Chat agent not running".into()),
    }
}

#[tauri::command]
pub fn mark_chat_read(
    state: State<'_, AppState>,
    conversation_id: Option<String>,
) -> Result<(), String> {
    let lock = state.chat_agent.lock().map_err(|e| e.to_string())?;
    if let Some(agent) = lock.as_ref() {
        agent.mark_read(conversation_id);
    }
    Ok(())
}

#[tauri::command]
pub fn get_chat_contacts(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let lock = state.chat_agent.lock().map_err(|e| e.to_string())?;
    let contacts = match lock.as_ref() {
        Some(agent) => agent.status().contacts,
        None => Vec::new(),
    };
    Ok(serde_json::json!({ "contacts": contacts }))
}

#[tauri::command]
pub fn get_chat_groups(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let lock = state.chat_agent.lock().map_err(|e| e.to_string())?;
    let groups = match lock.as_ref() {
        Some(agent) => agent.status().groups,
        None => Vec::new(),
    };
    Ok(serde_json::json!({ "groups": groups }))
}

#[tauri::command]
pub fn load_chat_conversation(
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<(), String> {
    let lock = state.chat_agent.lock().map_err(|e| e.to_string())?;
    if let Some(agent) = lock.as_ref() {
        agent.load_conversation(conversation_id);
    }
    Ok(())
}

#[tauri::command]
pub fn create_chat_group(
    state: State<'_, AppState>,
    name: String,
    member_ids: Vec<String>,
) -> Result<(), String> {
    let lock = state.chat_agent.lock().map_err(|e| e.to_string())?;
    match lock.as_ref() {
        Some(agent) => {
            agent.create_group(name, member_ids);
            Ok(())
        }
        None => Err("Chat agent not running".into()),
    }
}

/// Open the standalone chat window.
#[tauri::command]
pub fn open_chat_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    // If the window already exists, just show and focus it
    if let Some(win) = app.get_webview_window("chat") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    // Window is defined in tauri.conf.json but starts hidden;
    // it should already exist from the config. If it doesn't (e.g. was
    // closed), we create it dynamically.
    let _win = tauri::WebviewWindowBuilder::new(
        &app,
        "chat",
        tauri::WebviewUrl::App("chat-window.html".into()),
    )
    .title("Yomie Chat")
    .inner_size(700.0, 520.0)
    .min_inner_size(540.0, 400.0)
    .center()
    .build()
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Close the standalone chat window.
#[tauri::command]
pub fn close_chat_window(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("chat") {
        win.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Reconnect chat by stopping the current agent and starting a new one.
#[tauri::command]
pub fn reconnect_chat(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    // Stop existing chat agent
    {
        let lock = state.chat_agent.lock().map_err(|e| e.to_string())?;
        if let Some(agent) = lock.as_ref() {
            agent.stop();
        }
    }

    // Read current settings and device ID to build a new WS URL
    let settings = crate::config::Settings::load()
        .map_err(|e| e.to_string())?;
    let device_id = crate::identity::get_or_create_device_id()
        .map_err(|e| e.to_string())?;

    let ws_scheme = settings.server_ws_scheme();
    let ws_host = settings.server_ws_host();
    let chat_ws = format!("{}://{}/ws/chat/{}", ws_scheme, ws_host, device_id);

    let chat = crate::chat::ChatService::start(
        app,
        chat_ws,
        device_id,
    );

    let mut lock = state.chat_agent.lock().map_err(|e| e.to_string())?;
    *lock = Some(chat);

    Ok(())
}

// ---- Remote desktop ----

#[tauri::command]
pub fn get_remote_status(state: State<'_, AppState>) -> Result<crate::remote::RemoteStatus, String> {
    let lock = state.remote_agent.lock().map_err(|e| e.to_string())?;
    match lock.as_ref() {
        Some(agent) => Ok(agent.status()),
        None => Ok(crate::remote::RemoteStatus {
            active: false,
            standby: false,
            frame_count: 0,
            fps: 0.0,
            width: 0,
            height: 0,
            error: Some("Remote agent not running".into()),
        }),
    }
}

/// Start viewing a remote device by connecting to management WS.
/// Receives JPEG frames from the target agent and emits them to frontend.
#[tauri::command]
pub async fn start_remote_viewer(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    target_device_id: String,
) -> Result<(), String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let ws_url = settings.bd_api_url()
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let url = format!("{}/ws/bd-mgmt/{}", ws_url, target_device_id);

    log::info!("Starting remote viewer for {} at {}", target_device_id, url);

    tauri::async_runtime::spawn(async move {
        use tokio_tungstenite::{connect_async, tungstenite::Message};
        use futures_util::{SinkExt, StreamExt};

        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            connect_async(&url),
        ).await {
            Ok(Ok((ws, _))) => {
                let (mut write, mut read) = ws.split();

                // Send start command to trigger capture on target
                let start_cmd = serde_json::json!({ "type": "start" });
                if let Ok(txt) = serde_json::to_string(&start_cmd) {
                    let _ = write.send(Message::Text(txt.into())).await;
                }

                let _ = app.emit("remote-viewer-status", serde_json::json!({
                    "connected": true, "device_id": target_device_id
                }));

                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Binary(data)) => {
                            // JPEG frame — encode as base64 for frontend rendering
                            use base64::Engine;
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                            let _ = app.emit("remote-viewer-frame", b64);
                        }
                        Ok(Message::Text(txt)) => {
                            // JSON control message
                            let _ = app.emit("remote-viewer-message", txt.to_string());
                        }
                        Ok(Message::Close(_)) | Err(_) => break,
                        _ => {}
                    }
                }

                let _ = app.emit("remote-viewer-status", serde_json::json!({
                    "connected": false, "device_id": target_device_id
                }));
            }
            Ok(Err(e)) => {
                log::error!("Remote viewer connection failed: {}", e);
                let _ = app.emit("remote-viewer-status", serde_json::json!({
                    "connected": false, "error": e.to_string()
                }));
            }
            Err(_) => {
                log::error!("Remote viewer connection timed out");
                let _ = app.emit("remote-viewer-status", serde_json::json!({
                    "connected": false, "error": "Connection timed out"
                }));
            }
        }
    });

    Ok(())
}

// ---- Relay-based remote session (Phase 43) ----

/// Input event payload from the frontend for the relay session.
#[derive(Debug, Deserialize)]
pub struct RemoteInputPayload {
    #[serde(rename = "type")]
    pub kind: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub button: Option<u8>,
    pub delta_x: Option<i32>,
    pub delta_y: Option<i32>,
    pub key: Option<String>,
    pub modifiers: Option<Vec<String>>,
}

/// Start a relay-based remote session (H.264 decode via openh264).
///
/// Expects `connect_to_peer` + `authenticate` to have been called first.
/// Takes the relay connection from Session, bridges it into SessionManager
/// which handles video decode, input forwarding, clipboard sync, etc.
#[tauri::command]
pub async fn start_remote_session(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    peer_id: String,
) -> Result<(), String> {
    log::info!("Starting relay remote session for {}", peer_id);

    // 1. Extract Session handle (drop std::sync::Mutex guard before await)
    let session_handle = {
        let lock = state.session.lock().map_err(|e| e.to_string())?;
        lock.as_ref()
            .ok_or("No active session — call connect_to_peer first")?
            .clone_handle()
    };

    // 2. Take the relay connection out of Session (async — uses tokio::Mutex)
    let relay = session_handle
        .take_relay()
        .await
        .ok_or("No relay connection — is the session authenticated?")?;

    // 3. Bridge relay into mpsc channels for SessionManager
    let (msg_tx, msg_rx) = relay.into_channels();

    // 4. Create and start SessionManager
    let session_mgr = SessionManager::start(app.clone(), msg_rx, msg_tx)
        .map_err(|e| format!("Failed to start session manager: {}", e))?;

    // 5. Store in AppState
    {
        let mut mgr_lock = state.session_manager.lock().map_err(|e| e.to_string())?;
        *mgr_lock = Some(session_mgr);
    }

    let _ = app.emit("remote-viewer-status", serde_json::json!({
        "connected": true, "device_id": peer_id, "mode": "relay"
    }));

    log::info!("Relay remote session active for {}", peer_id);
    Ok(())
}

/// Stop the relay-based remote session.
#[tauri::command]
pub async fn stop_remote_session(
    state: State<'_, AppState>,
) -> Result<(), String> {
    log::info!("Stopping relay remote session");
    // Stop SessionManager
    {
        let mut mgr_lock = state.session_manager.lock().map_err(|e| e.to_string())?;
        if let Some(mgr) = mgr_lock.take() {
            mgr.stop();
        }
    }
    // Disconnect the session
    {
        let session_lock = state.session.lock().map_err(|e| e.to_string())?;
        if let Some(session) = session_lock.as_ref() {
            session.disconnect();
        }
    }
    Ok(())
}

/// Send an input event (mouse/key/wheel) through the SessionManager relay.
#[tauri::command]
pub async fn send_remote_input(
    state: State<'_, AppState>,
    input: RemoteInputPayload,
) -> Result<(), String> {
    let mgr_lock = state.session_manager.lock().map_err(|e| e.to_string())?;
    let mgr = mgr_lock.as_ref().ok_or("No active remote session")?;
    let mods = input.modifiers.clone().unwrap_or_default();

    let cmd = match input.kind.as_str() {
        "mouse_move" => SessionCommand::MouseMove {
            x: input.x.unwrap_or(0),
            y: input.y.unwrap_or(0),
        },
        "mouse_down" => SessionCommand::MouseButton {
            x: input.x.unwrap_or(0),
            y: input.y.unwrap_or(0),
            button: input.button.unwrap_or(0),
            down: true,
            modifiers: mods,
        },
        "mouse_up" => SessionCommand::MouseButton {
            x: input.x.unwrap_or(0),
            y: input.y.unwrap_or(0),
            button: input.button.unwrap_or(0),
            down: false,
            modifiers: mods,
        },
        "wheel" => SessionCommand::MouseWheel {
            x: input.x.unwrap_or(0),
            y: input.y.unwrap_or(0),
            delta_x: input.delta_x.unwrap_or(0),
            delta_y: input.delta_y.unwrap_or(0),
        },
        "key_down" => SessionCommand::Key {
            key: input.key.clone().unwrap_or_default(),
            down: true,
            modifiers: mods,
        },
        "key_up" => SessionCommand::Key {
            key: input.key.clone().unwrap_or_default(),
            down: false,
            modifiers: mods,
        },
        "refresh_video" => SessionCommand::RefreshVideo,
        other => {
            log::debug!("Unknown remote input type: {}", other);
            return Ok(());
        }
    };

    mgr.send(cmd);
    Ok(())
}

/// Toggle clipboard sync for the active remote session.
#[tauri::command]
pub async fn toggle_clipboard_sync(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mgr_lock = state.session_manager.lock().map_err(|e| e.to_string())?;
    if let Some(mgr) = mgr_lock.as_ref() {
        mgr.send(SessionCommand::ToggleClipboard { enabled });
    }
    Ok(())
}

/// Send a special key (Ctrl+Alt+Del, LockScreen).
#[tauri::command]
pub async fn send_special_key(
    state: State<'_, AppState>,
    key: String,
) -> Result<(), String> {
    let mgr_lock = state.session_manager.lock().map_err(|e| e.to_string())?;
    if let Some(mgr) = mgr_lock.as_ref() {
        mgr.send(SessionCommand::SpecialKey { key });
    }
    Ok(())
}

/// Switch to a different display on the remote machine.
#[tauri::command]
pub async fn switch_display(
    state: State<'_, AppState>,
    index: i32,
) -> Result<(), String> {
    let mgr_lock = state.session_manager.lock().map_err(|e| e.to_string())?;
    if let Some(mgr) = mgr_lock.as_ref() {
        mgr.send(SessionCommand::SwitchDisplay { index });
    }
    Ok(())
}

/// Set video quality options.
#[tauri::command]
pub async fn set_quality(
    state: State<'_, AppState>,
    image_quality: String,
    fps: u32,
) -> Result<(), String> {
    let mgr_lock = state.session_manager.lock().map_err(|e| e.to_string())?;
    if let Some(mgr) = mgr_lock.as_ref() {
        mgr.send(SessionCommand::SetQuality { image_quality, fps });
    }
    Ok(())
}

/// Toggle session recording.
#[tauri::command]
pub async fn toggle_recording(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    let mgr_lock = state.session_manager.lock().map_err(|e| e.to_string())?;
    if let Some(mgr) = mgr_lock.as_ref() {
        mgr.send(SessionCommand::ToggleRecording { enabled });
    }
    Ok(())
}

/// Get session quality stats.
#[tauri::command]
pub async fn get_session_quality() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "fps": 0.0,
        "latency_ms": 0,
        "bandwidth_kbps": 0.0,
        "codec": "none",
        "width": 0,
        "height": 0,
    }))
}

// ---- Device management ----

#[tauri::command]
pub fn get_device_info_cmd() -> Result<crate::management::DeviceInfo, String> {
    Ok(crate::management::get_device_info())
}

#[tauri::command]
pub fn lock_screen_cmd() -> Result<(), String> {
    if !crate::service::is_elevated() {
        return Err("Admin elevation required".into());
    }
    crate::management::lock_screen().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn logoff_user_cmd() -> Result<(), String> {
    if !crate::service::is_elevated() {
        return Err("Admin elevation required".into());
    }
    crate::management::logoff_user().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn restart_system_cmd(delay_secs: u32) -> Result<(), String> {
    if !crate::service::is_elevated() {
        return Err("Admin elevation required".into());
    }
    crate::management::restart_system(delay_secs).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn shutdown_system_cmd(delay_secs: u32) -> Result<(), String> {
    if !crate::service::is_elevated() {
        return Err("Admin elevation required".into());
    }
    crate::management::shutdown_system(delay_secs).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn abort_shutdown_cmd() -> Result<(), String> {
    crate::management::abort_shutdown().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn run_predefined_cmd(
    cmd: crate::management::PredefinedCommand,
) -> Result<crate::management::CommandResult, String> {
    if !crate::service::is_elevated() {
        return Err("Admin elevation required".into());
    }
    crate::management::run_predefined(cmd).map_err(|e| e.to_string())
}

// ---- CDAP Desktop Agent ----

/// Get CDAP configuration from settings file.
#[tauri::command]
pub fn cdap_get_config() -> Result<CdapConfig, String> {
    CdapConfig::load().map_err(|e| e.to_string())
}

/// Save CDAP configuration.
#[tauri::command]
pub fn cdap_save_config(config: CdapConfig) -> Result<(), String> {
    config.save().map_err(|e| e.to_string())
}

/// Start the CDAP agent (connect to gateway).
#[tauri::command]
pub async fn cdap_connect(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> Result<CdapStatus, String> {
    let mut lock = state.cdap_agent.lock().map_err(|e| e.to_string())?;

    // Stop existing agent if running
    if let Some(agent) = lock.take() {
        agent.stop();
    }

    let config = CdapConfig::load().map_err(|e| e.to_string())?;
    if config.gateway_url.is_empty() {
        return Err("CDAP gateway URL not configured".into());
    }

    let agent = CdapAgent::start(config, app_handle);
    let status = agent.status();
    *lock = Some(agent);
    Ok(status)
}

/// Stop the CDAP agent.
#[tauri::command]
pub fn cdap_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    let mut lock = state.cdap_agent.lock().map_err(|e| e.to_string())?;
    if let Some(agent) = lock.take() {
        agent.stop();
    }
    Ok(())
}

/// Get current CDAP agent status.
#[tauri::command]
pub fn cdap_status(state: State<'_, AppState>) -> Result<CdapStatus, String> {
    let lock = state.cdap_agent.lock().map_err(|e| e.to_string())?;
    match lock.as_ref() {
        Some(agent) => Ok(agent.status()),
        None => Ok(CdapStatus {
            connected: false,
            device_id: String::new(),
            gateway_url: String::new(),
            uptime_secs: 0,
            heartbeat_count: 0,
            active_sessions: vec![],
            last_error: Some("CDAP agent not started".into()),
        }),
    }
}

// ---- Branding ----

/// Get current branding (company name, accent color, support contact).
#[tauri::command]
pub fn get_branding(
    branding_state: State<'_, crate::tray::BrandingState>,
) -> Result<crate::tray::Branding, String> {
    let branding = branding_state.0.lock().map_err(|e| e.to_string())?;
    Ok(branding.clone())
}

// ---- Help Request ----

/// Send a help request to the server.  Operators get a real-time notification.
#[tauri::command]
pub async fn request_help(
    state: State<'_, AppState>,
    message: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();
    let device_id = identity::get_or_create_device_id().map_err(|e| e.to_string())?;
    let hostname = whoami::devicename();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "device_id": device_id,
        "hostname": hostname,
        "message": message,
        "timestamp": chrono::Utc::now().timestamp_millis(),
    });

    let url = format!("{}/api/bd/help-request", base_url);

    let resp = client
        .post(&url)
        .header("X-Device-Id", &device_id)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Help request failed: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Help request failed (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).map_err(|e| format!("Invalid response: {}", e))
}

// ---- Operator Mode ----

async fn operator_json_request(
    state: &State<'_, AppState>,
    access_token: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
    timeout_secs: u64,
    action: &str,
) -> Result<serde_json::Value, String> {
    let base_url = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.bd_api_url()
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}{}", base_url, path);
    let mut request = client
        .request(method, &url)
        .header("Authorization", format!("Bearer {}", access_token));

    if let Some(payload) = body {
        request = request.json(&payload);
    }

    let resp = request
        .send()
        .await
        .map_err(|e| format!("{}: {}", action, e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("{} (HTTP {}): {}", action, status, text));
    }

    if text.trim().is_empty() {
        return Ok(serde_json::json!({ "success": true }));
    }

    serde_json::from_str(&text)
        .or_else(|_| Ok(serde_json::json!({ "success": true, "raw": text })))
}

// ---- API Proxy ----
// Routes all web panel HTTP requests through Rust's reqwest client,
// bypassing CORS and mixed-content restrictions in Tauri's WebView.
// The shared `http_client` has a cookie store — session cookies are
// automatically persisted across calls, enabling express-session auth.

/// Proxy an HTTP request to the Yomie web panel through Rust.
///
/// This solves the "Failed to fetch" problem caused by Tauri WebView
/// (`https://tauri.localhost`) being blocked from fetching `http://` URLs
/// (mixed-content policy). The reqwest client supports both HTTP and HTTPS.
#[tauri::command]
pub async fn api_proxy(
    state: State<'_, AppState>,
    server_url: String,
    path: String,
    method: String,
    body: Option<String>,
) -> Result<serde_json::Value, String> {
    let normalized_path = if path.starts_with('/') { path } else { format!("/{}", path) };
    let url = format!("{}{}", server_url.trim_end_matches('/'), normalized_path);

    let client = &state.http_client;

    let builder = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        "DELETE" => client.delete(&url),
        other => return Err(format!("Unsupported HTTP method: {}", other)),
    };

    // Identify as Tauri client so the Node.js CSRF middleware skips validation
    let builder = builder
        .header("Content-Type", "application/json")
        .header("Origin", "https://tauri.localhost");

    let builder = if let Some(ref b) = body {
        builder.body(b.clone())
    } else {
        builder
    };

    log::info!("[api_proxy] {} {} (body_len={})", method.to_uppercase(), url, body.as_ref().map_or(0, |b| b.len()));

    let resp = builder
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    log::info!("[api_proxy] {} {} → {} (resp_len={})", method.to_uppercase(), normalized_path, status, text.len());

    // Parse response — always return a JSON object with __status for the frontend
    let mut json_value: serde_json::Value = if text.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(&text).unwrap_or_else(|_| serde_json::json!({ "raw": text }))
    };

    // Attach HTTP status code so frontend can detect 401/403/etc.
    if let Some(obj) = json_value.as_object_mut() {
        obj.insert("__status".to_string(), serde_json::json!(status));
    }

    if status >= 400 {
        // Return error string for Tauri invoke error handling
        let error_msg = json_value
            .get("error")
            .or_else(|| json_value.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Request failed");
        log::warn!("[api_proxy] ERROR {} {} → HTTP {}: {}", method.to_uppercase(), normalized_path, status, error_msg);
        return Err(format!("HTTP {}: {}", status, error_msg));
    }

    Ok(json_value)
}

/// Clear the cookie store in the shared HTTP client.
/// Called on logout to wipe session cookies.
#[tauri::command]
pub async fn api_clear_session(
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Replace the cookie store by rebuilding the display — reqwest 0.12
    // does not expose `.cookie_store_mut()`.  The simplest approach is
    // to re-create nothing: cookies expire on their own when we logout
    // on the server side.  The next login creates a fresh session.
    // We still call the logout endpoint first from the frontend.
    let _ = &state.http_client; // no-op placeholder

    // Clear the stored access token on logout
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.access_token = None;
    let _ = settings.save();

    Ok(())
}

/// Store an access token obtained from operator login.
///
/// This is called by the frontend after a successful login via `api_proxy`
/// to persist the token for use by `BdRelayConnection::connect()`.
#[tauri::command]
pub fn set_access_token(
    state: State<'_, AppState>,
    token: String,
) -> Result<(), String> {
    log::info!("[set_access_token] token_len={} token_prefix={}",
        token.len(),
        if token.len() >= 8 { &token[..8] } else { &token });
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.access_token = if token.is_empty() { None } else { Some(token) };
    settings.save().map_err(|e| format!("Failed to save access token: {}", e))
}

/// Login as an operator and get an access token.
///
/// Calls the dedicated BD operator login endpoint which returns a persistent
/// access token. The token is stored securely (OS keyring) and used for
/// relay connections and operator API calls.
///
/// Retries up to 3 times with exponential backoff on HTTP 429 (rate limit).
#[tauri::command]
pub async fn operator_login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<serde_json::Value, String> {
    let base_url = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.bd_api_url()
    };

    let device_id = identity::get_or_create_device_id().map_err(|e| e.to_string())?;
    let client = &state.http_client;

    log::info!("[operator_login] url={}/api/bd/operator/login user={} device_id={}", base_url, username, device_id);

    let body = serde_json::json!({
        "username": username,
        "password": password,
        "device_id": device_id,
    });

    let url = format!("{}/api/bd/operator/login", base_url);

    // Retry on 429 (brute-force lockout) — the session login that runs just
    // before this call will clear the lockout server-side, but the response
    // may arrive before the server processes the clearance.
    let max_retries: u32 = 3;
    let mut last_err = String::new();

    for attempt in 0..=max_retries {
        if attempt > 0 {
            let delay = std::time::Duration::from_secs(2u64.pow(attempt));
            log::info!("[operator_login] retry {}/{} after {}s", attempt, max_retries, delay.as_secs());
            tokio::time::sleep(delay).await;
        }

        let resp = match client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Origin", "https://tauri.localhost")
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                log::error!("[operator_login] request failed: {}", e);
                last_err = format!("Login failed: {}", e);
                continue;
            }
        };

        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;

        log::info!("[operator_login] response: status={} body_len={} body_prefix={}", status, text.len(), &text[..text.len().min(100)]);

        if status.as_u16() == 429 && attempt < max_retries {
            log::warn!("[operator_login] HTTP 429 (rate limited), will retry");
            last_err = format!("Login failed (HTTP {}): {}", status, text);
            continue;
        }

        if !status.is_success() {
            log::warn!("[operator_login] HTTP {} — {}", status, &text[..text.len().min(200)]);
            return Err(format!("Login failed (HTTP {}): {}", status, text));
        }

        let json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("Invalid login response: {}", e))?;

        if let Some(token) = json.get("access_token").and_then(|v| v.as_str()) {
            log::info!("[operator_login] token obtained, len={}", token.len());
            let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
            settings.access_token = Some(token.to_string());
            settings.save().map_err(|e| format!("Failed to save access token: {}", e))?;
        } else {
            log::warn!("[operator_login] response has no access_token field");
        }

        return Ok(json);
    }

    Err(last_err)
}

/// Get list of devices visible to the operator.
#[tauri::command]
pub async fn operator_get_devices(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/peers", base_url);

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch devices: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Failed to fetch devices (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).map_err(|e| format!("Invalid device list response: {}", e))
}

/// Complete 2FA login step for operator accounts with TOTP enabled.
#[tauri::command]
pub async fn operator_login_2fa(
    state: State<'_, AppState>,
    partial_token: String,
    totp_code: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "partial_token": partial_token,
        "totp_code": totp_code,
    });

    let url = format!("{}/api/auth/login/2fa", base_url);

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("2FA verification failed: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("2FA verification failed (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).map_err(|e| format!("Invalid 2FA response: {}", e))
}

/// Get help requests visible to the operator.
#[tauri::command]
pub async fn operator_get_help_requests(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/bd/help-requests", base_url);

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch help requests: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Failed to fetch help requests (HTTP {}): {}", status, text));
    }

    let payload: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Invalid help request response: {}", e))?;

    let requests = payload
        .get("requests")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut request| {
            if request.get("timestamp").is_none() {
                if let Some(created_at) = request.get("created_at").cloned() {
                    request["timestamp"] = created_at;
                }
            }
            request
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({ "requests": requests }))
}

/// Accept a pending help request and mark it as in-progress.
#[tauri::command]
pub async fn operator_accept_help_request(
    state: State<'_, AppState>,
    access_token: String,
    request_id: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/bd/help-requests/{}/accept", base_url, request_id);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to accept help request: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Failed to accept help request (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).map_err(|e| format!("Invalid response: {}", e))
}

#[tauri::command]
pub async fn operator_record_session_event(
    state: State<'_, AppState>,
    access_token: String,
    device_id: String,
    hostname: String,
    action: String,
    session_id: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "device_id": device_id,
        "hostname": hostname,
        "action": action,
        "session_id": session_id,
    });

    let url = format!("{}/api/bd/operator/sessions", base_url);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to record session event: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Failed to record session event (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text)
        .or_else(|_| Ok(serde_json::json!({ "success": true })))
}

/// Get device groups/folders (synced with Node.js panel grouping).
#[tauri::command]
pub async fn operator_get_device_groups(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/peers?with_tags=true", base_url);

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch device groups: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Failed to fetch device groups (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).map_err(|e| format!("Invalid groups response: {}", e))
}

/// Configure a remote device (set notes, tags, folder assignment).
#[tauri::command]
pub async fn operator_configure_device(
    state: State<'_, AppState>,
    access_token: String,
    device_id: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/peers/{}", base_url, device_id);

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to get device config: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Failed to get device config (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).map_err(|e| format!("Invalid config response: {}", e))
}

/// Install a module on a remote device.
#[tauri::command]
pub async fn operator_install_module(
    state: State<'_, AppState>,
    access_token: String,
    device_id: String,
    module_name: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "module_name": module_name,
    });

    let url = format!("{}/api/bd/mgmt/{}/send", base_url, device_id);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to install module: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Module installation failed (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).map_err(|e| format!("Invalid response: {}", e))
}

// ---- File Transfer ----

#[tauri::command]
pub fn browse_local_files(path: String, show_hidden: bool) -> Result<serde_json::Value, String> {
    let listing = crate::file_transfer::FileBrowser::list_dir(&path, show_hidden)
        .map_err(|e| e.to_string())?;
    serde_json::to_value(&listing).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_file_native(path: String) -> Result<(), String> {
    open::that(&path).map_err(|e| format!("Failed to open {}: {}", path, e))
}

// ---- Activity Tracking ----

#[tauri::command]
pub fn get_activity_log(state: State<'_, AppState>) -> Result<Vec<ActivityEntry>, String> {
    let tracker = state.activity.lock().map_err(|e| e.to_string())?;
    Ok(tracker.get_recent(100))
}

// ---- Device Actions (Phase 44) ----

#[tauri::command]
pub async fn operator_send_device_action(
    state: State<'_, AppState>,
    access_token: String,
    device_id: String,
    action: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let body = serde_json::json!({
        "action": action,
    });

    let url = format!("{}/api/bd/mgmt/{}/send", base_url, device_id);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to send action '{}': {}", action, e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Action '{}' failed (HTTP {}): {}", action, status, text));
    }

    serde_json::from_str(&text).or_else(|_| Ok(serde_json::json!({ "success": true })))
}

#[tauri::command]
pub async fn operator_wake_on_lan(
    state: State<'_, AppState>,
    access_token: String,
    device_id: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/peers/{}/wol", base_url, device_id);

    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to send WOL: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("WOL failed (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).or_else(|_| Ok(serde_json::json!({ "success": true })))
}

#[tauri::command]
pub async fn operator_get_session_history(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    let settings = {
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        s.clone()
    };

    let base_url = settings.bd_api_url();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("{}/api/bd/operator/sessions?limit=100", base_url);

    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await
        .map_err(|e| format!("Failed to fetch session history: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;

    if !status.is_success() {
        return Err(format!("Session history failed (HTTP {}): {}", status, text));
    }

    serde_json::from_str(&text).map_err(|e| format!("Invalid session history response: {}", e))
}

// ---- Automation & Alerts ----

#[tauri::command]
pub async fn operator_automation_get_rules(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        "/api/automation/rules",
        None,
        10,
        "Failed to fetch automation rules",
    )
    .await
}

#[tauri::command]
pub async fn operator_automation_save_rule(
    state: State<'_, AppState>,
    access_token: String,
    rule: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut payload = rule;
    let maybe_id = payload.get("id").and_then(|value| value.as_i64());
    if let Some(object) = payload.as_object_mut() {
        object.remove("id");
    }

    let (method, path, action) = if let Some(id) = maybe_id {
        (
            reqwest::Method::PATCH,
            format!("/api/automation/rules/{}", id),
            "Failed to update automation rule",
        )
    } else {
        (
            reqwest::Method::POST,
            "/api/automation/rules".to_string(),
            "Failed to create automation rule",
        )
    };

    operator_json_request(
        &state,
        &access_token,
        method,
        &path,
        Some(payload),
        10,
        action,
    )
    .await
}

#[tauri::command]
pub async fn operator_automation_delete_rule(
    state: State<'_, AppState>,
    access_token: String,
    rule_id: i64,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/automation/rules/{}", rule_id);
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::DELETE,
        &path,
        None,
        10,
        "Failed to delete automation rule",
    )
    .await
}

#[tauri::command]
pub async fn operator_automation_get_alerts(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        "/api/automation/alerts?limit=50",
        None,
        10,
        "Failed to fetch automation alerts",
    )
    .await
}

#[tauri::command]
pub async fn operator_automation_ack_alert(
    state: State<'_, AppState>,
    access_token: String,
    alert_id: i64,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/automation/alerts/{}/ack", alert_id);
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::POST,
        &path,
        None,
        10,
        "Failed to acknowledge automation alert",
    )
    .await
}

#[tauri::command]
pub async fn operator_automation_get_commands(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        "/api/automation/commands?limit=50",
        None,
        10,
        "Failed to fetch automation commands",
    )
    .await
}

#[tauri::command]
pub async fn operator_automation_create_command(
    state: State<'_, AppState>,
    access_token: String,
    command: serde_json::Value,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::POST,
        "/api/automation/commands",
        Some(command),
        15,
        "Failed to queue automation command",
    )
    .await
}

// ---- DataGuard ----

#[tauri::command]
pub async fn operator_dataguard_get_policies(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        "/api/dataguard/policies",
        None,
        10,
        "Failed to fetch DataGuard policies",
    )
    .await
}

#[tauri::command]
pub async fn operator_dataguard_save_policy(
    state: State<'_, AppState>,
    access_token: String,
    policy: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut payload = policy;
    let maybe_id = payload.get("id").and_then(|value| value.as_i64());
    if let Some(object) = payload.as_object_mut() {
        object.remove("id");
    }

    let (method, path, action) = if let Some(id) = maybe_id {
        (
            reqwest::Method::PATCH,
            format!("/api/dataguard/policies/{}", id),
            "Failed to update DataGuard policy",
        )
    } else {
        (
            reqwest::Method::POST,
            "/api/dataguard/policies".to_string(),
            "Failed to create DataGuard policy",
        )
    };

    operator_json_request(
        &state,
        &access_token,
        method,
        &path,
        Some(payload),
        10,
        action,
    )
    .await
}

#[tauri::command]
pub async fn operator_dataguard_delete_policy(
    state: State<'_, AppState>,
    access_token: String,
    policy_id: i64,
) -> Result<serde_json::Value, String> {
    let path = format!("/api/dataguard/policies/{}", policy_id);
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::DELETE,
        &path,
        None,
        10,
        "Failed to delete DataGuard policy",
    )
    .await
}

#[tauri::command]
pub async fn operator_dataguard_get_events(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        "/api/dataguard/events?limit=50",
        None,
        10,
        "Failed to fetch DataGuard events",
    )
    .await
}

#[tauri::command]
pub async fn operator_dataguard_get_stats(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        "/api/dataguard/stats",
        None,
        10,
        "Failed to fetch DataGuard stats",
    )
    .await
}

// ---------------------------------------------------------------------------
//  Server Management Commands (MGMT Client)
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn server_get_health(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let base_url = {
        let settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.bd_api_url()
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}/server/stats", base_url);
    let resp = client.get(&url).send().await.map_err(|e| format!("Health fetch failed: {}", e))?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn server_get_clients(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        "/api/peers?online=true",
        None,
        10,
        "Failed to fetch connected clients",
    )
    .await
}

#[tauri::command]
pub async fn server_get_operators(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        "/api/users",
        None,
        10,
        "Failed to fetch operators",
    )
    .await
}

#[tauri::command]
pub async fn server_get_audit(
    state: State<'_, AppState>,
    access_token: String,
    filter: String,
) -> Result<serde_json::Value, String> {
    let path = if filter == "all" {
        "/api/audit?limit=100".to_string()
    } else {
        format!("/api/audit?limit=100&category={}", filter)
    };
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        &path,
        None,
        10,
        "Failed to fetch audit log",
    )
    .await
}

#[tauri::command]
pub async fn server_get_api_keys(
    state: State<'_, AppState>,
    access_token: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::GET,
        "/api/keys",
        None,
        10,
        "Failed to fetch API keys",
    )
    .await
}

#[tauri::command]
pub async fn server_disconnect_client(
    state: State<'_, AppState>,
    access_token: String,
    peer_id: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::POST,
        &format!("/api/peers/{}/disconnect", peer_id),
        None,
        10,
        "Failed to disconnect client",
    )
    .await
}

#[tauri::command]
pub async fn server_ban_client(
    state: State<'_, AppState>,
    access_token: String,
    peer_id: String,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::POST,
        &format!("/api/peers/{}/ban", peer_id),
        None,
        10,
        "Failed to ban client",
    )
    .await
}

#[tauri::command]
pub async fn server_revoke_api_key(
    state: State<'_, AppState>,
    access_token: String,
    key_id: i64,
) -> Result<serde_json::Value, String> {
    operator_json_request(
        &state,
        &access_token,
        reqwest::Method::DELETE,
        &format!("/api/keys/{}", key_id),
        None,
        10,
        "Failed to revoke API key",
    )
    .await
}

// ---------------------------------------------------------------------------
//  Notification Commands (MGMT Client)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_notifications(
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    // Read dismissed / read sets from AppState
    let dismissed = state.dismissed_notifs.lock().map_err(|e| e.to_string())?;
    let read_set = state.read_notifs.lock().map_err(|e| e.to_string())?;

    let tracker = state.activity.lock().map_err(|e| e.to_string())?;
    let recent = tracker.get_recent(50);
    // Map activity entries to notification format
    let notifs: Vec<serde_json::Value> = recent.iter().enumerate().filter_map(|(i, e)| {
        let id = format!("notif-{}", i);
        if dismissed.contains(&id) { return None; }
        Some(serde_json::json!({
            "id": id,
            "type": match e.action.as_str() {
                "help_request" => "help_request",
                "login" | "login_failed" => "alert",
                "connect" | "disconnect" => "connection",
                "chat" => "chat",
                _ => "system",
            },
            "title": e.action,
            "message": e.details,
            "timestamp": chrono::NaiveDateTime::parse_from_str(&e.timestamp, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.and_utc().timestamp_millis())
                .unwrap_or(0),
            "read": read_set.contains(&id),
            "device_id": if e.target.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(e.target.clone()) },
        }))
    }).collect();
    Ok(serde_json::json!(notifs))
}

#[tauri::command]
pub fn mark_notification_read(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<(), String> {
    let mut read_set = state.read_notifs.lock().map_err(|e| e.to_string())?;
    read_set.insert(notification_id);
    Ok(())
}

#[tauri::command]
pub fn mark_all_notifications_read(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let tracker = state.activity.lock().map_err(|e| e.to_string())?;
    let recent = tracker.get_recent(50);
    let mut read_set = state.read_notifs.lock().map_err(|e| e.to_string())?;
    for i in 0..recent.len() {
        read_set.insert(format!("notif-{}", i));
    }
    Ok(())
}

#[tauri::command]
pub fn dismiss_notification(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<(), String> {
    let mut dismissed = state.dismissed_notifs.lock().map_err(|e| e.to_string())?;
    dismissed.insert(notification_id);
    Ok(())
}

// ---- Logging bridge (frontend → Rust log file) ----

/// Write a log line from the frontend (JS/TS) into the shared log file.
///
/// Levels: "debug", "info", "warn", "error".  Unknown → info.
#[tauri::command]
pub fn write_log(level: String, module: String, message: String) -> Result<(), String> {
    let lvl = match level.to_lowercase().as_str() {
        "debug" | "trace" => log::Level::Debug,
        "warn" | "warning" => log::Level::Warn,
        "error" => log::Level::Error,
        _ => log::Level::Info,
    };
    crate::logging::write_frontend_log(lvl, &module, &message);
    Ok(())
}

/// Return the log file path so the UI can display it.
#[tauri::command]
pub fn get_log_path() -> Result<String, String> {
    crate::logging::log_file_path()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Log file not initialized".to_string())
}

/// Open the log file in the OS default text editor / file manager.
#[tauri::command]
pub fn open_log_file() -> Result<(), String> {
    let path = crate::logging::log_file_path()
        .ok_or("Log file not initialized")?;
    open::that(&path).map_err(|e| format!("Failed to open log: {}", e))
}
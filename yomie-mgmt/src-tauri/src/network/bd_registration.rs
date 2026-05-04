//! Yomie native registration — Go server-centric enrollment.
//!
//! Registers directly with the Go server (:21114) via:
//! 1. POST /api/devices/register  — initial enrollment request
//! 2. GET  /api/devices/register/status — poll until approved
//! 3. POST /api/heartbeat + /api/sysinfo — ongoing heartbeat
//!
//! Also maintains a secondary heartbeat with the Node.js console (:5000)
//! for backward compatibility with existing panel features.
//!
//! ## Enrollment flow
//!
//! ```text
//! Client                         Go Server           Web Panel
//!   |                               |                    |
//!   |-- POST /api/devices/register ->                    |
//!   |                               |                    |
//!   |<-- status: "approved"/"pending"                    |
//!   |                               |                    |
//!   |   (if pending)                |                    |
//!   |-- GET /api/devices/register/status (poll) ->       |
//!   |                               |  <-- operator approves
//!   |<-- status: "approved", sync_mode, branding         |
//!   |                               |                    |
//!   |   (approved)                  |                    |
//!   |-- POST /api/heartbeat ------->                     |
//!   |-- POST /api/sysinfo --------->                     |
//!   |   (repeat every N seconds)    |                    |
//! ```

use anyhow::{Context, Result};
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::{self, header::{HeaderName, HeaderValue}};

use crate::config::Settings;

// ---------------------------------------------------------------------------
//  Constants
// ---------------------------------------------------------------------------

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);
const RECONNECT_DELAY: Duration = Duration::from_secs(5);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
/// How often to poll enrollment status while pending.
const PENDING_POLL_INTERVAL: Duration = Duration::from_secs(3);
const MGMT_SIGNATURE_VERSION: &str = "bd-mgmt-v1";

// ---------------------------------------------------------------------------
//  API types — Go server enrollment
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct EnrollmentRequest {
    device_id: String,
    uuid: String,
    hostname: String,
    platform: String,
    version: String,
    device_type: String,
    public_key: Option<String>,
    token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct EnrollmentResponse {
    pub status: String,        // "approved", "pending", "rejected", "unknown"
    pub device_id: String,
    #[serde(default)]
    pub server_time: i64,
    #[serde(default)]
    pub sync_mode: Option<String>,      // "silent", "standard", "turbo"
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub branding: Option<BrandingConfig>,
    #[serde(default)]
    pub server_key: Option<String>,     // Ed25519 pubkey (base64)
    #[serde(default)]
    pub heartbeat_interval: Option<u64>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BrandingConfig {
    pub company_name: String,
    pub accent_color: String,
    #[serde(default)]
    pub support_contact: String,
    #[serde(default)]
    pub colors: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub sync_modes: Vec<SyncModeOption>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SyncModeOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct HeartbeatRequest {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cpu: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    memory: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    disk: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct HeartbeatResponse {
    success: bool,
    #[serde(default)]
    pending_connections: Vec<PendingConnection>,
    #[serde(default)]
    server_time: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PendingConnection {
    pub session_id: String,
    pub initiator_id: String,
    pub created_at: u64,
}

// Signal WebSocket messages
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignalMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

// ---------------------------------------------------------------------------
//  Registration status
// ---------------------------------------------------------------------------

/// Enrollment phase — tracks where we are in the Go server enrollment flow.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum EnrollmentPhase {
    /// Not yet attempted registration.
    Idle,
    /// Connecting to Go server.
    Connecting,
    /// POST /api/devices/register sent, waiting for response.
    Registering,
    /// Server returned "pending" — waiting for operator approval.
    PendingApproval,
    /// Enrollment approved — starting sync.
    Approved,
    /// Syncing initial data to server.
    Syncing,
    /// Fully operational — heartbeat running.
    Active,
    /// Enrollment was rejected.
    Rejected,
    /// Error during any phase.
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct BdRegistrationStatus {
    pub registered: bool,
    pub device_id: String,
    pub server_url: String,
    pub heartbeat_count: u64,
    pub signal_connected: bool,
    pub last_error: Option<String>,
    pub pending_connections: Vec<PendingConnection>,
    /// Current enrollment phase.
    pub phase: EnrollmentPhase,
    /// Operator-assigned sync mode after approval.
    pub sync_mode: Option<String>,
    /// Branding config received from server.
    pub branding: Option<BrandingConfig>,
    /// Operator-assigned display name.
    pub display_name: Option<String>,
}

// ---------------------------------------------------------------------------
//  Service handle
// ---------------------------------------------------------------------------

pub struct BdRegistrationService {
    status_rx: watch::Receiver<BdRegistrationStatus>,
    cancel_tx: watch::Sender<bool>,
    /// Channel to receive incoming connection notifications
    pub incoming_rx: mpsc::Receiver<PendingConnection>,
}

impl BdRegistrationService {
    /// Start the Yomie native registration service.
    pub fn start(settings: &Settings, device_id: &str) -> Self {
        let go_api_url = settings.go_api_url();
        let console_url = settings.bd_api_url();

        let initial_status = BdRegistrationStatus {
            registered: false,
            device_id: device_id.to_string(),
            server_url: go_api_url.clone(),
            heartbeat_count: 0,
            signal_connected: false,
            last_error: None,
            pending_connections: Vec::new(),
            phase: EnrollmentPhase::Idle,
            sync_mode: None,
            branding: None,
            display_name: None,
        };

        let (status_tx, status_rx) = watch::channel(initial_status);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (incoming_tx, incoming_rx) = mpsc::channel(32);

        let device_id = device_id.to_string();
        let token = settings.access_token.clone().unwrap_or_default();

        tauri::async_runtime::spawn(async move {
            bd_registration_loop(
                go_api_url, console_url, device_id, token,
                status_tx, cancel_rx, incoming_tx,
            ).await;
        });

        BdRegistrationService {
            status_rx,
            cancel_tx,
            incoming_rx,
        }
    }

    pub fn status(&self) -> BdRegistrationStatus {
        self.status_rx.borrow().clone()
    }

    pub fn stop(&self) {
        let _ = self.cancel_tx.send(true);
        info!("Yomie registration service stop requested");
    }
}

// ---------------------------------------------------------------------------
//  Main loop — Go server-centric enrollment
// ---------------------------------------------------------------------------

#[allow(unused_assignments)]
async fn bd_registration_loop(
    go_api_url: String,
    console_url: String,
    device_id: String,
    token: String,
    status_tx: watch::Sender<BdRegistrationStatus>,
    mut cancel_rx: watch::Receiver<bool>,
    incoming_tx: mpsc::Sender<PendingConnection>,
) {
    // MGMT-C3: gate self-signed cert acceptance behind BETTERDESK_STRICT_TLS env var.
    let strict_tls = matches!(
        std::env::var("BETTERDESK_STRICT_TLS").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES")
    );
    let mut cb = reqwest::Client::builder().timeout(HTTP_TIMEOUT);
    if !strict_tls {
        cb = cb.danger_accept_invalid_certs(true);
    }
    let client = cb.build().expect("Failed to create HTTP client");

    let mut heartbeat_count: u64 = 0;
    let mut sysinfo_sent = false;
    let mut current_sync_mode: Option<String> = None;
    let mut current_branding: Option<BrandingConfig> = None;
    let mut current_display_name: Option<String> = None;

    loop {
        if *cancel_rx.borrow() {
            info!("BD registration cancelled");
            return;
        }

        // ── PHASE 1: Enrollment with Go server ──────────────────────────
        info!("Enrolling with Go server at {}", go_api_url);

        let _ = status_tx.send(BdRegistrationStatus {
            registered: false,
            device_id: device_id.clone(),
            server_url: go_api_url.clone(),
            heartbeat_count,
            signal_connected: false,
            last_error: None,
            pending_connections: Vec::new(),
            phase: EnrollmentPhase::Connecting,
            sync_mode: None,
            branding: None,
            display_name: None,
        });

        match do_enroll(&client, &go_api_url, &device_id, &token).await {
            Ok(resp) => {
                match resp.status.as_str() {
                    "approved" => {
                        info!("Enrollment approved immediately (open mode)");
                        current_sync_mode = resp.sync_mode.clone();
                        current_branding = resp.branding.clone();
                        current_display_name = resp.display_name.clone();

                        let _ = status_tx.send(BdRegistrationStatus {
                            registered: true,
                            device_id: device_id.clone(),
                            server_url: go_api_url.clone(),
                            heartbeat_count,
                            signal_connected: false,
                            last_error: None,
                            pending_connections: Vec::new(),
                            phase: EnrollmentPhase::Approved,
                            sync_mode: current_sync_mode.clone(),
                            branding: current_branding.clone(),
                            display_name: current_display_name.clone(),
                        });
                    }

                    "pending" => {
                        info!("Enrollment pending — waiting for operator approval");
                        let _ = status_tx.send(BdRegistrationStatus {
                            registered: false,
                            device_id: device_id.clone(),
                            server_url: go_api_url.clone(),
                            heartbeat_count,
                            signal_connected: false,
                            last_error: None,
                            pending_connections: Vec::new(),
                            phase: EnrollmentPhase::PendingApproval,
                            sync_mode: None,
                            branding: None,
                            display_name: None,
                        });

                        // ── PHASE 2: Poll until approved ────────────────
                        let approved = poll_until_approved(
                            &client, &go_api_url, &device_id,
                            &status_tx, &mut cancel_rx,
                        ).await;

                        match approved {
                            Some(approval) => {
                                current_sync_mode = approval.sync_mode.clone();
                                current_branding = approval.branding.clone();
                                current_display_name = approval.display_name.clone();
                            }
                            None => {
                                // Cancelled or rejected — retry from scratch
                                if *cancel_rx.borrow() { return; }
                                continue;
                            }
                        }
                    }

                    "rejected" => {
                        let msg = resp.message.unwrap_or_else(|| "Enrollment rejected".into());
                        warn!("Enrollment rejected: {}", msg);
                        let _ = status_tx.send(BdRegistrationStatus {
                            registered: false,
                            device_id: device_id.clone(),
                            server_url: go_api_url.clone(),
                            heartbeat_count,
                            signal_connected: false,
                            last_error: Some(msg),
                            pending_connections: Vec::new(),
                            phase: EnrollmentPhase::Rejected,
                            sync_mode: None,
                            branding: None,
                            display_name: None,
                        });
                        // Wait longer before retry on rejection
                        cancellable_sleep(Duration::from_secs(60), &mut cancel_rx).await;
                        continue;
                    }

                    other => {
                        warn!("Unexpected enrollment status: {}", other);
                        cancellable_sleep(RECONNECT_DELAY, &mut cancel_rx).await;
                        continue;
                    }
                }
            }
            Err(e) => {
                warn!("Enrollment failed: {}", e);
                let _ = status_tx.send(BdRegistrationStatus {
                    registered: false,
                    device_id: device_id.clone(),
                    server_url: go_api_url.clone(),
                    heartbeat_count,
                    signal_connected: false,
                    last_error: Some(e.to_string()),
                    pending_connections: Vec::new(),
                    phase: EnrollmentPhase::Error,
                    sync_mode: None,
                    branding: None,
                    display_name: None,
                });
                cancellable_sleep(RECONNECT_DELAY, &mut cancel_rx).await;
                if *cancel_rx.borrow() { return; }
                continue;
            }
        }

        // ── PHASE 3: Syncing — send initial sysinfo ─────────────────────
        let _ = status_tx.send(BdRegistrationStatus {
            registered: true,
            device_id: device_id.clone(),
            server_url: go_api_url.clone(),
            heartbeat_count,
            signal_connected: false,
            last_error: None,
            pending_connections: Vec::new(),
            phase: EnrollmentPhase::Syncing,
            sync_mode: current_sync_mode.clone(),
            branding: current_branding.clone(),
            display_name: current_display_name.clone(),
        });

        if !sysinfo_sent {
            send_go_sysinfo(&client, &go_api_url, &device_id).await;
            sysinfo_sent = true;
        }

        // Also register with Node.js console (secondary, for panel features)
        do_console_register(&client, &console_url, &device_id, &token).await;

        // ── PHASE 3.5: Connect management WebSocket ─────────────────────
        // This replaces CDAP for desktop clients — the Go server manages
        // real-time features (remote start, revocation, config push) over
        // a lightweight WS channel on the API port (21114).
		let mgmt_ws_url = {
			let scheme = if go_api_url.starts_with("https://") { "wss" } else { "ws" };
            let base = go_api_url
                .trim_start_matches("https://")
                .trim_start_matches("http://");
            format!("{}://{}/ws/bd-mgmt/{}", scheme, base, device_id)
        };
        let mgmt_handle = spawn_mgmt_ws(mgmt_ws_url, device_id.clone(), status_tx.clone());

        // ── PHASE 4: Active — heartbeat loop ────────────────────────────
        let _ = status_tx.send(BdRegistrationStatus {
            registered: true,
            device_id: device_id.clone(),
            server_url: go_api_url.clone(),
            heartbeat_count,
            signal_connected: false,
            last_error: None,
            pending_connections: Vec::new(),
            phase: EnrollmentPhase::Active,
            sync_mode: current_sync_mode.clone(),
            branding: current_branding.clone(),
            display_name: current_display_name.clone(),
        });

        info!("Entering heartbeat loop (sync_mode={:?})", current_sync_mode);

        loop {
            let interval = heartbeat_interval_for_mode(current_sync_mode.as_deref());

            tokio::select! {
                _ = sleep(interval) => {
                    // Go server heartbeat (primary)
                    match send_go_heartbeat(&client, &go_api_url, &device_id).await {
                        HeartbeatResult::Ok => {}
                        HeartbeatResult::Revoked => {
                            warn!("Device revoked — wiping local secrets and disconnecting");
                            mgmt_handle.abort();
                            wipe_local_secrets();
                            let _ = status_tx.send(BdRegistrationStatus {
                                registered: false,
                                device_id: device_id.clone(),
                                server_url: go_api_url.clone(),
                                heartbeat_count,
                                signal_connected: false,
                                last_error: Some("Device revoked by administrator".into()),
                                pending_connections: Vec::new(),
                                phase: EnrollmentPhase::Error,
                                sync_mode: None,
                                branding: None,
                                display_name: None,
                            });
                            return;
                        }
                        HeartbeatResult::Error => {}
                    }
                    heartbeat_count += 1;

                    // Console heartbeat (secondary, best-effort)
                    do_console_heartbeat(&client, &console_url, &device_id, &token, &incoming_tx).await;

                    let _ = status_tx.send(BdRegistrationStatus {
                        registered: true,
                        device_id: device_id.clone(),
                        server_url: go_api_url.clone(),
                        heartbeat_count,
                        signal_connected: false,
                        last_error: None,
                        pending_connections: Vec::new(),
                        phase: EnrollmentPhase::Active,
                        sync_mode: current_sync_mode.clone(),
                        branding: current_branding.clone(),
                        display_name: current_display_name.clone(),
                    });

                    debug!("Heartbeat OK (count={}, mode={:?})", heartbeat_count, current_sync_mode);
                }
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        info!("Heartbeat loop cancelled");
                        mgmt_handle.abort();
                        return;
                    }
                }
            }
        }
    }
}

/// Poll `/api/devices/register/status` until the device is approved or rejected.
/// Returns `Some(EnrollmentResponse)` on approval, `None` on rejection or cancel.
async fn poll_until_approved(
    client: &reqwest::Client,
    go_api_url: &str,
    device_id: &str,
    status_tx: &watch::Sender<BdRegistrationStatus>,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Option<EnrollmentResponse> {
    loop {
        if *cancel_rx.borrow() { return None; }

        tokio::select! {
            _ = sleep(PENDING_POLL_INTERVAL) => {}
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() { return None; }
            }
        }

        let url = format!("{}/api/devices/register/status?device_id={}",
            go_api_url, urlencoding::encode(device_id));

        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => {
                if let Ok(data) = resp.json::<EnrollmentResponse>().await {
                    match data.status.as_str() {
                        "approved" => {
                            info!("Enrollment approved by operator!");
                            return Some(data);
                        }
                        "rejected" => {
                            warn!("Enrollment rejected by operator");
                            let _ = status_tx.send(BdRegistrationStatus {
                                registered: false,
                                device_id: device_id.to_string(),
                                server_url: go_api_url.to_string(),
                                heartbeat_count: 0,
                                signal_connected: false,
                                last_error: Some("Rejected by operator".into()),
                                pending_connections: Vec::new(),
                                phase: EnrollmentPhase::Rejected,
                                sync_mode: None,
                                branding: None,
                                display_name: None,
                            });
                            return None;
                        }
                        "pending" => {
                            debug!("Still pending approval...");
                        }
                        _ => {
                            debug!("Unexpected poll status: {}", data.status);
                        }
                    }
                }
            }
            Ok(resp) => {
                debug!("Poll status HTTP {}", resp.status());
            }
            Err(e) => {
                debug!("Poll status failed: {}", e);
            }
        }
    }
}

/// Return the heartbeat interval based on sync mode.
fn heartbeat_interval_for_mode(mode: Option<&str>) -> Duration {
    match mode {
        Some("turbo") => Duration::from_secs(5),
        Some("silent") => Duration::from_secs(30),
        _ => HEARTBEAT_INTERVAL, // standard = 15s
    }
}

// ---------------------------------------------------------------------------
//  Cancellation helper
// ---------------------------------------------------------------------------

async fn cancellable_sleep(dur: Duration, cancel_rx: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = sleep(dur) => false,
        _ = cancel_rx.changed() => *cancel_rx.borrow(),
    }
}

// ---------------------------------------------------------------------------
//  HTTP helpers — Go server (primary)
// ---------------------------------------------------------------------------

/// Enroll with the Go server.
async fn do_enroll(
    client: &reqwest::Client,
    go_api_url: &str,
    device_id: &str,
    token: &str,
) -> Result<EnrollmentResponse> {
    let hostname = whoami::devicename();
    let platform = std::env::consts::OS.to_string();

    let body = EnrollmentRequest {
        device_id: device_id.to_string(),
        uuid: get_uuid()?,
        hostname,
        platform,
        version: env!("CARGO_PKG_VERSION").to_string(),
        device_type: "yomie".to_string(),
        public_key: get_public_key_base64().ok(),
        token: if token.is_empty() { None } else { Some(token.to_string()) },
    };

    let resp = client
        .post(format!("{}/api/devices/register", go_api_url))
        .json(&body)
        .send()
        .await
        .context("Enrollment HTTP request failed")?;

    // Accept 200, 202 (pending), and even 403 (rejected) as valid JSON responses
    let status = resp.status();
    if status.is_success() || status.as_u16() == 202 || status.as_u16() == 403 {
        resp.json::<EnrollmentResponse>()
            .await
            .context("Failed to parse enrollment response")
    } else {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Enrollment HTTP {}: {}", status, text);
    }
}

// ---------------------------------------------------------------------------
//  HTTP helpers — Node.js console (secondary / backward compat)
// ---------------------------------------------------------------------------

/// Register with Node.js console (best-effort, non-blocking on failure).
async fn do_console_register(
    client: &reqwest::Client,
    console_url: &str,
    device_id: &str,
    token: &str,
) {
    let hostname = whoami::devicename();
    let platform = std::env::consts::OS.to_string();

    let body = serde_json::json!({
        "device_id": device_id,
        "hostname": hostname,
        "platform": platform,
        "version": env!("CARGO_PKG_VERSION"),
    });

    let mut req = client
        .post(format!("{}/api/bd/register", console_url))
        .json(&body);

    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    } else {
        req = req.header("X-Device-Id", device_id);
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            debug!("Console register OK");
        }
        Ok(resp) => {
            debug!("Console register HTTP {}", resp.status());
        }
        Err(e) => {
            debug!("Console register failed (non-critical): {}", e);
        }
    }
}

/// Heartbeat to Node.js console (best-effort).
async fn do_console_heartbeat(
    client: &reqwest::Client,
    console_url: &str,
    device_id: &str,
    token: &str,
    incoming_tx: &mpsc::Sender<PendingConnection>,
) {
    let body = serde_json::json!({ "device_id": device_id });

    let mut req = client
        .post(format!("{}/api/bd/heartbeat", console_url))
        .json(&body);

    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    } else {
        req = req.header("X-Device-Id", device_id);
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(data) = resp.json::<HeartbeatResponse>().await {
                for pc in data.pending_connections {
                    let _ = incoming_tx.send(pc).await;
                }
            }
        }
        _ => {
            // Non-critical — Go server is the primary heartbeat target
        }
    }
}

// ---------------------------------------------------------------------------
//  Identity helpers
// ---------------------------------------------------------------------------

fn get_uuid() -> Result<String> {
    let config_dir = directories::ProjectDirs::from("com", "yomie", "Yomie")
        .context("Failed to determine config directory")?
        .config_dir()
        .to_path_buf();

    let uuid_path = config_dir.join("uuid");
    if uuid_path.exists() {
        let bytes = std::fs::read(&uuid_path)?;
        if bytes.len() == 16 {
            let uuid = uuid::Uuid::from_bytes(bytes.try_into().unwrap());
            return Ok(uuid.to_string());
        }
    }

    let uuid = uuid::Uuid::new_v4();
    std::fs::create_dir_all(&config_dir).ok();
    std::fs::write(&uuid_path, uuid.as_bytes())?;
    Ok(uuid.to_string())
}

fn get_public_key_base64() -> Result<String> {
    let config_dir = directories::ProjectDirs::from("com", "yomie", "Yomie")
        .context("Failed to determine config directory")?
        .config_dir()
        .to_path_buf();

    let pk_path = config_dir.join("id_ed25519.pub");
    if pk_path.exists() {
        let bytes = std::fs::read(&pk_path)?;
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &bytes,
        ))
    } else {
        anyhow::bail!("No public key found")
    }
}

fn get_identity_dir() -> Result<std::path::PathBuf> {
    directories::ProjectDirs::from("com", "yomie", "Yomie")
        .context("Failed to determine config directory")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

fn load_or_create_signing_key() -> Result<SigningKey> {
    use rand::rngs::OsRng;
    let config_dir = get_identity_dir()?;
    let pk_path = config_dir.join("id_ed25519.pub");
    let sk_path = config_dir.join("id_ed25519");

    if sk_path.exists() && pk_path.exists() {
        let raw = std::fs::read(&sk_path).context("Failed to read device signing key")?;
        let key_bytes: [u8; 32] = raw
            .as_slice()
            .try_into()
            .context("Invalid Ed25519 signing key length")?;
        return Ok(SigningKey::from_bytes(&key_bytes));
    }

    if sk_path.exists() != pk_path.exists() {
        anyhow::bail!("Device identity files are incomplete — remove stale key files and re-enroll")
    }

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    std::fs::create_dir_all(&config_dir).ok();
    std::fs::write(&sk_path, signing_key.to_bytes()).context("Failed to write device signing key")?;
    std::fs::write(&pk_path, verifying_key.to_bytes()).context("Failed to write device public key")?;
    info!("Generated Ed25519 keypair for Yomie management authentication");
    Ok(signing_key)
}

fn build_mgmt_ws_request(url: &str, device_id: &str) -> Result<http::Request<()>> {
    let signing_key = load_or_create_signing_key()?;
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let nonce = uuid::Uuid::new_v4().to_string();
    let payload = format!("{}\n{}\n{}\n{}", MGMT_SIGNATURE_VERSION, device_id, timestamp, nonce);
    let signature = signing_key.sign(payload.as_bytes());
    let signature_b64 = base64::engine::general_purpose::STANDARD.encode(signature.to_bytes());

    let mut request = url.into_client_request().context("Failed to build management WS request")?;
    request.headers_mut().insert(
        HeaderName::from_static("x-bd-timestamp"),
        HeaderValue::from_str(&timestamp).context("Invalid timestamp header")?,
    );
    request.headers_mut().insert(
        HeaderName::from_static("x-bd-nonce"),
        HeaderValue::from_str(&nonce).context("Invalid nonce header")?,
    );
    request.headers_mut().insert(
        HeaderName::from_static("x-bd-signature"),
        HeaderValue::from_str(&signature_b64).context("Invalid signature header")?,
    );
    Ok(request)
}

// ---------------------------------------------------------------------------
//  Go server helpers (port 21114)
// ---------------------------------------------------------------------------

/// Send a heartbeat to the Go server so the device appears in the peers table.
/// Heartbeat result — used to detect revocation from server.
enum HeartbeatResult {
    Ok,
    Revoked,
    Error,
}

async fn send_go_heartbeat(client: &reqwest::Client, go_api_url: &str, device_id: &str) -> HeartbeatResult {
    let body = serde_json::json!({ "id": device_id });
    match client
        .post(format!("{}/api/heartbeat", go_api_url))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            debug!("Go server heartbeat OK for {}", device_id);
            HeartbeatResult::Ok
        }
        Ok(resp) if resp.status().as_u16() == 403 || resp.status().as_u16() == 404 => {
            warn!("Device {} revoked by server (HTTP {})", device_id, resp.status());
            HeartbeatResult::Revoked
        }
        Ok(resp) => {
            debug!("Go server heartbeat HTTP {}", resp.status());
            HeartbeatResult::Error
        }
        Err(e) => {
            debug!("Go server heartbeat failed: {}", e);
            HeartbeatResult::Error
        }
    }
}

/// Send sysinfo to the Go server (hostname, OS, version).
///
/// This populates the hostname/platform columns in the device list.
/// Only needs to be sent once per session — the Go server caches it.
async fn send_go_sysinfo(client: &reqwest::Client, go_api_url: &str, device_id: &str) {
    let hostname = whoami::devicename();
    let platform = std::env::consts::OS.to_string();
    let version = env!("CARGO_PKG_VERSION").to_string();
    let os_version = whoami::distro();

    let body = serde_json::json!({
        "id": device_id,
        "hostname": hostname,
        "os": format!("{} {}", platform, os_version),
        "version": version,
    });

    match client
        .post(format!("{}/api/sysinfo", go_api_url))
        .json(&body)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            info!("Go server sysinfo sent for {} ({})", device_id, hostname);
        }
        Ok(resp) => {
            debug!("Go server sysinfo HTTP {}", resp.status());
        }
        Err(e) => {
            debug!("Go server sysinfo failed: {}", e);
        }
    }
}

/// Wipe locally stored secrets when the device is revoked by the server.
///
/// Removes access tokens, keys, and settings related to server connection.
/// The device ID itself is preserved (it's a hardware-derived identifier).
fn wipe_local_secrets() {
    use crate::config::Settings;

    info!("Wiping local secrets due to device revocation");

    match Settings::load() {
        Ok(mut settings) => {
            settings.access_token = None;
            settings.server_key = String::new();
            // Clear console/server URLs to force re-setup
            settings.console_url = String::new();
            settings.server_address = String::new();
            if let Err(e) = settings.save() {
                warn!("Failed to save wiped settings: {}", e);
            } else {
                info!("Local secrets wiped — device must re-enroll");
            }
        }
        Err(e) => {
            warn!("Failed to load settings for wipe: {}", e);
        }
    }

    // Remove key files if they exist
    if let Ok(config_path) = Settings::config_path() {
        let key_dir = config_path.parent().unwrap_or(std::path::Path::new("."));
        for file_name in &["id_ed25519", "id_ed25519.pub"] {
            let path = key_dir.join(file_name);
            if path.exists() {
                if let Err(e) = std::fs::remove_file(&path) {
                    warn!("Failed to remove {}: {}", path.display(), e);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
//  Management WebSocket — replaces CDAP for desktop clients
// ---------------------------------------------------------------------------

/// Spawn a lightweight management WebSocket that connects to the Go server.
///
/// This runs as a background task alongside the heartbeat loop. It provides:
/// - Server-side liveness detection (WS ping/pong)
/// - Real-time management commands from operators
/// - Revocation signal push
///
/// Returns a JoinHandle that can be aborted when the heartbeat loop exits.
fn spawn_mgmt_ws(
    url: String,
	device_id: String,
    _status_tx: watch::Sender<BdRegistrationStatus>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut reconnect_delay = Duration::from_secs(2);
        let max_delay = Duration::from_secs(60);

        loop {
            info!("Management WS: connecting to {}", url);

            let request = match build_mgmt_ws_request(&url, &device_id) {
                Ok(req) => req,
                Err(e) => {
                    warn!("Management WS: failed to build signed request: {}", e);
                    sleep(reconnect_delay).await;
                    reconnect_delay = (reconnect_delay * 2).min(max_delay);
                    continue;
                }
            };

            match tokio_tungstenite::connect_async(request).await {
                Ok((ws_stream, _)) => {
                    info!("Management WS: connected");
                    reconnect_delay = Duration::from_secs(2);

                    use futures_util::{SinkExt, StreamExt};
                    use tokio_tungstenite::tungstenite::Message as WsMsg;
                    let (mut write, mut read) = ws_stream.split();

                    loop {
                        tokio::select! {
                            msg = read.next() => {
                                match msg {
                                    Some(Ok(WsMsg::Text(text))) => {
                                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                                            let msg_type = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                            match msg_type {
                                                "welcome" => {
                                                    info!("Management WS: server acknowledged");
                                                }
                                                "ping" => {
                                                    let pong = serde_json::json!({"type": "pong"});
                                                    let _ = write.send(WsMsg::Text(pong.to_string())).await;
                                                }
                                                "revoke" => {
                                                    warn!("Management WS: device revoked by server");
                                                    wipe_local_secrets();
                                                    return;
                                                }
                                                "remote-start" => {
                                                    info!("Management WS: remote session requested by operator");
                                                    // TODO: trigger remote agent start via Tauri event
                                                }
                                                "config-push" => {
                                                    info!("Management WS: config update from server");
                                                    // TODO: apply pushed configuration
                                                }
                                                _ => {
                                                    debug!("Management WS: unknown message type: {}", msg_type);
                                                }
                                            }
                                        }
                                    }
                                    Some(Ok(WsMsg::Close(_))) => {
                                        info!("Management WS: server closed connection");
                                        break;
                                    }
                                    Some(Err(e)) => {
                                        debug!("Management WS: read error: {}", e);
                                        break;
                                    }
                                    None => break,
                                    _ => {} // Binary, Ping, Pong — ignore
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Management WS: connection failed: {}", e);
                }
            }

            info!("Management WS: reconnecting in {:?}", reconnect_delay);
            sleep(reconnect_delay).await;
            reconnect_delay = (reconnect_delay * 2).min(max_delay);
        }
    })
}

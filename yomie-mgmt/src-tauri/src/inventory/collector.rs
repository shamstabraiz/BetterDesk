//! Phased inventory collector with incremental sync and history.
//!
//! Instead of collecting everything at startup, the collector progresses
//! through lightweight → full sync in stages, uploading partial data at
//! each step. After a complete snapshot is assembled, it switches to
//! incremental mode where only changes are detected and uploaded.
//!
//! ## Sync phases (startup)
//!
//! | Delay    | Phase         | What is collected               |
//! |----------|---------------|---------------------------------|
//! | T+2 s    | Lightweight   | CPU %, RAM %, uptime            |
//! | T+5 s    | BasicHw       | hostname, OS, arch, cores, RAM  |
//! | T+15 s   | CpuDetailed   | brand, vendor, freq (200 ms)    |
//! | T+30 s   | Storage       | disk enumeration                |
//! | T+45 s   | Network       | network interfaces              |
//! | T+90 s   | Software      | installed app list (heavy)      |
//! | then     | Incremental   | periodic partial refreshes      |
//!
//! ## Incremental intervals
//!
//! | Data          | Interval |
//! |---------------|----------|
//! | CPU / RAM     | 30 s     |
//! | Disks         | 5 min    |
//! | Network       | 5 min    |
//! | Software      | 6 h      |

use anyhow::{Context, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tokio::sync::watch;
use tokio::time::{sleep, Duration, Instant};

use crate::config::Settings;
use super::diff;
use super::hardware::{self, HardwareInfo};
use super::history::{SyncHistory, SyncSnapshot};
use super::software::{self, SoftwareList};

// ---------------------------------------------------------------------------
//  Phase timing constants (Standard mode — used as defaults)
// ---------------------------------------------------------------------------

/// Delay before Phase 1 (Lightweight) starts.
const PHASE_LIGHTWEIGHT_DELAY: Duration = Duration::from_secs(2);
/// Delay before Phase 2 (BasicHw).
const PHASE_BASIC_DELAY: Duration = Duration::from_secs(3);
/// Delay before Phase 3 (CpuDetailed).
const PHASE_CPU_DELAY: Duration = Duration::from_secs(10);
/// Delay before Phase 4 (Storage).
const PHASE_STORAGE_DELAY: Duration = Duration::from_secs(15);
/// Delay before Phase 5 (Network).
const PHASE_NETWORK_DELAY: Duration = Duration::from_secs(15);
/// Delay before Phase 6 (Software).
const PHASE_SOFTWARE_DELAY: Duration = Duration::from_secs(45);

// ---------------------------------------------------------------------------
//  Incremental interval constants (Standard mode)
// ---------------------------------------------------------------------------

/// Telemetry (CPU/RAM) refresh interval in incremental mode.
const INCR_TELEMETRY: Duration = Duration::from_secs(30);
/// Disk/network refresh interval in incremental mode.
const INCR_STORAGE_NETWORK: Duration = Duration::from_secs(300);
/// Software refresh interval in incremental mode.
const INCR_SOFTWARE: Duration = Duration::from_secs(6 * 3600);
/// Full re-upload interval (ensures server has complete data periodically).
const INCR_FULL_UPLOAD: Duration = Duration::from_secs(900);

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);

/// How often to persist history to disk.
const HISTORY_SAVE_INTERVAL: Duration = Duration::from_secs(120);

// ---------------------------------------------------------------------------
//  Sync mode — operator-assigned speed tier
// ---------------------------------------------------------------------------

/// Sync speed mode assigned by operator during enrollment approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncMode {
    /// Minimal telemetry — CPU/RAM every 60s, no software scan.
    Silent,
    /// Balanced — 30s telemetry, 5min disk/network, 6h software.
    Standard,
    /// Aggressive — 10s telemetry, 1min disk/network, 30min software.
    Turbo,
}

impl Default for SyncMode {
    fn default() -> Self {
        Self::Standard
    }
}

impl SyncMode {
    /// Parse from string (e.g., from server enrollment response).
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "silent" => Self::Silent,
            "turbo" => Self::Turbo,
            _ => Self::Standard,
        }
    }

    /// Telemetry (CPU/RAM) refresh interval.
    pub fn telemetry_interval(&self) -> Duration {
        match self {
            Self::Silent => Duration::from_secs(60),
            Self::Standard => INCR_TELEMETRY,
            Self::Turbo => Duration::from_secs(10),
        }
    }

    /// Disk/network refresh interval.
    pub fn storage_network_interval(&self) -> Duration {
        match self {
            Self::Silent => Duration::from_secs(600),
            Self::Standard => INCR_STORAGE_NETWORK,
            Self::Turbo => Duration::from_secs(60),
        }
    }

    /// Software list refresh interval.
    pub fn software_interval(&self) -> Duration {
        match self {
            Self::Silent => Duration::from_secs(0), // disabled
            Self::Standard => INCR_SOFTWARE,
            Self::Turbo => Duration::from_secs(1800),
        }
    }

    /// Full re-upload interval.
    pub fn full_upload_interval(&self) -> Duration {
        match self {
            Self::Silent => Duration::from_secs(3600),
            Self::Standard => INCR_FULL_UPLOAD,
            Self::Turbo => Duration::from_secs(300),
        }
    }

    /// Phase delay multiplier — turbo runs phases faster.
    pub fn phase_delay_multiplier(&self) -> f32 {
        match self {
            Self::Silent => 2.0,   // slower startup
            Self::Standard => 1.0,
            Self::Turbo => 0.3,    // 3x faster startup phases
        }
    }
}

// ---------------------------------------------------------------------------
//  Sync phases
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SyncPhase {
    /// Waiting for initial delay.
    Idle,
    /// Phase 1: CPU %, RAM %, uptime.
    Lightweight,
    /// Phase 2: hostname, OS, arch, core counts, total RAM.
    BasicHw,
    /// Phase 3: CPU brand, vendor, frequency (200 ms measurement).
    CpuDetailed,
    /// Phase 4: disk enumeration.
    Storage,
    /// Phase 5: network interface enumeration.
    Network,
    /// Phase 6: installed software list.
    Software,
    /// All phases complete — periodic incremental sync.
    Incremental,
}

impl std::fmt::Display for SyncPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Lightweight => write!(f, "lightweight"),
            Self::BasicHw => write!(f, "basic_hw"),
            Self::CpuDetailed => write!(f, "cpu_detailed"),
            Self::Storage => write!(f, "storage"),
            Self::Network => write!(f, "network"),
            Self::Software => write!(f, "software"),
            Self::Incremental => write!(f, "incremental"),
        }
    }
}

// ---------------------------------------------------------------------------
//  API types
// ---------------------------------------------------------------------------

/// Full inventory payload sent to the console.
#[derive(Debug, Clone, Serialize)]
pub struct InventoryPayload {
    pub device_id: String,
    pub hardware: HardwareInfo,
    pub software: SoftwareList,
    pub collected_at: String,
    /// Current sync phase (informational for the server).
    pub sync_phase: String,
}

/// Lightweight telemetry payload (CPU + RAM only).
#[derive(Debug, Clone, Serialize)]
pub struct TelemetryPayload {
    pub device_id: String,
    pub cpu_usage_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub uptime_secs: u64,
    pub timestamp: String,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    success: bool,
    #[serde(default)]
    error: Option<String>,
}

// ---------------------------------------------------------------------------
//  Collector status (exposed to frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct InventoryStatus {
    pub running: bool,
    pub phase: SyncPhase,
    pub last_upload_at: Option<String>,
    pub upload_count: u64,
    pub last_error: Option<String>,
    pub hardware: Option<HardwareInfo>,
    pub history_size: usize,
    /// Active sync mode (from enrollment approval).
    pub sync_mode: String,
}

// ---------------------------------------------------------------------------
//  Service handle
// ---------------------------------------------------------------------------

pub struct InventoryCollector {
    status_rx: watch::Receiver<InventoryStatus>,
    cancel_tx: watch::Sender<bool>,
}

impl InventoryCollector {
    /// Start the inventory collector background service.
    pub fn start(settings: &Settings, device_id: &str) -> Self {
        Self::start_with_mode(settings, device_id, SyncMode::Standard)
    }

    /// Start the inventory collector with a specific sync mode.
    pub fn start_with_mode(settings: &Settings, device_id: &str, mode: SyncMode) -> Self {
        let initial = InventoryStatus {
            running: true,
            phase: SyncPhase::Idle,
            last_upload_at: None,
            upload_count: 0,
            last_error: None,
            hardware: None,
            history_size: 0,
            sync_mode: format!("{:?}", mode),
        };

        let (status_tx, status_rx) = watch::channel(initial);
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let base_url = settings.bd_api_url();
        let token = settings.access_token.clone().unwrap_or_default();
        let device_id = device_id.to_string();

        tauri::async_runtime::spawn(async move {
            collector_loop(base_url, device_id, token, mode, status_tx, cancel_rx).await;
        });

        InventoryCollector {
            status_rx,
            cancel_tx,
        }
    }

    /// Get current status snapshot.
    pub fn status(&self) -> InventoryStatus {
        self.status_rx.borrow().clone()
    }

    /// Stop the collector.
    pub fn stop(&self) {
        let _ = self.cancel_tx.send(true);
        info!("Inventory collector stop requested");
    }
}

// ---------------------------------------------------------------------------
//  Cancellation helper
// ---------------------------------------------------------------------------

/// Sleep for `dur`, returning `true` if cancelled during the wait.
async fn cancellable_sleep(dur: Duration, cancel_rx: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = sleep(dur) => false,
        _ = cancel_rx.changed() => *cancel_rx.borrow(),
    }
}

// ---------------------------------------------------------------------------
//  Main loop
// ---------------------------------------------------------------------------

async fn collector_loop(
    base_url: String,
    device_id: String,
    token: String,
    mode: SyncMode,
    status_tx: watch::Sender<InventoryStatus>,
    mut cancel_rx: watch::Receiver<bool>,
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

    let _mode_str = format!("{:?}", mode);
    let mul = mode.phase_delay_multiplier();
    let mut upload_count: u64 = 0;
    let mut history = SyncHistory::load();

    info!("Phased inventory collector started for device {} (mode={:?}, mul={:.1}x)",
        device_id, mode, mul);

    // Accumulated snapshot — built up across phases.
    let mut hw = HardwareInfo::default();
    let mut sw = SoftwareList::default();

    // Shared System object for lightweight telemetry (avoids recreation).
    let mut sys = System::new_with_specifics(
        RefreshKind::new()
            .with_cpu(CpuRefreshKind::new().with_cpu_usage())
            .with_memory(MemoryRefreshKind::everything()),
    );

    // Phase delay helper — applies sync mode multiplier.
    let phase_delay = |base: Duration| -> Duration {
        Duration::from_secs_f32(base.as_secs_f32() * mul)
    };

    // ── PHASE 1: Lightweight ──────────────────────────────────────────────
    if cancellable_sleep(phase_delay(PHASE_LIGHTWEIGHT_DELAY), &mut cancel_rx).await {
        return;
    }
    info!("Sync phase: Lightweight");
    update_status(&status_tx, SyncPhase::Lightweight, upload_count, None, &hw, history.len());

    let metrics = {
        let s = &mut sys;
        tokio::task::spawn_blocking({
            // We need to move sys into the blocking task temporarily.
            // Use a channel to shuttle it across.
            let mut sys_take = unsafe_take_sys(s);
            move || {
                let m = hardware::collect_lightweight(&mut sys_take);
                (m, sys_take)
            }
        })
        .await
        .unwrap_or_else(|e| {
            warn!("Lightweight collection panicked: {}", e);
            (hardware::LightweightMetrics::default(), System::new())
        })
    };
    // Put sys back.
    sys = metrics.1;
    let lw = metrics.0;

    hw.cpu.usage_percent = lw.cpu_usage_percent;
    hw.memory.used_bytes = lw.memory_used_bytes;
    hw.memory.total_bytes = lw.memory_total_bytes;
    hw.uptime_secs = lw.uptime_secs;

    upload_telemetry_quiet(&client, &base_url, &token, &device_id, &lw).await;
    record_phase(&mut history, "lightweight", &hw, None, &[]);
    info!("Phase Lightweight complete — CPU {:.1}%, RAM {:.0} MB",
        lw.cpu_usage_percent,
        lw.memory_used_bytes as f64 / 1_048_576.0
    );

    // ── PHASE 2: Basic hardware ───────────────────────────────────────────
    if cancellable_sleep(phase_delay(PHASE_BASIC_DELAY), &mut cancel_rx).await {
        return;
    }
    info!("Sync phase: BasicHw");
    update_status(&status_tx, SyncPhase::BasicHw, upload_count, None, &hw, history.len());

    let basic = tokio::task::spawn_blocking(hardware::collect_basic)
        .await
        .unwrap_or_default();

    hw.hostname = basic.hostname;
    hw.os_name = basic.os_name;
    hw.os_version = basic.os_version;
    hw.kernel_version = basic.kernel_version;
    hw.architecture = basic.architecture;
    hw.uptime_secs = basic.uptime_secs;
    hw.cpu.physical_cores = basic.physical_cores;
    hw.cpu.logical_cores = basic.logical_cores;
    hw.memory.total_bytes = basic.total_memory_bytes;
    hw.memory.swap_total_bytes = basic.total_swap_bytes;

    record_phase(&mut history, "basic_hw", &hw, None, &[]);
    info!("Phase BasicHw complete — {} / {} / {}",
        hw.hostname, hw.os_name, hw.architecture
    );

    // ── PHASE 3: CPU detailed ─────────────────────────────────────────────
    if cancellable_sleep(phase_delay(PHASE_CPU_DELAY), &mut cancel_rx).await {
        return;
    }
    info!("Sync phase: CpuDetailed");
    update_status(&status_tx, SyncPhase::CpuDetailed, upload_count, None, &hw, history.len());

    let cpu = tokio::task::spawn_blocking(hardware::collect_cpu_detailed)
        .await
        .unwrap_or_default();
    hw.cpu = cpu;

    record_phase(&mut history, "cpu_detailed", &hw, None, &[]);
    info!("Phase CpuDetailed complete — {} @ {} MHz",
        hw.cpu.brand, hw.cpu.frequency_mhz
    );

    // ── PHASE 4: Storage ──────────────────────────────────────────────────
    if cancellable_sleep(phase_delay(PHASE_STORAGE_DELAY), &mut cancel_rx).await {
        return;
    }
    info!("Sync phase: Storage");
    update_status(&status_tx, SyncPhase::Storage, upload_count, None, &hw, history.len());

    let disks = tokio::task::spawn_blocking(hardware::collect_storage)
        .await
        .unwrap_or_default();
    hw.disks = disks;

    record_phase(&mut history, "storage", &hw, None, &[]);
    info!("Phase Storage complete — {} disks", hw.disks.len());

    // ── PHASE 5: Network ──────────────────────────────────────────────────
    if cancellable_sleep(phase_delay(PHASE_NETWORK_DELAY), &mut cancel_rx).await {
        return;
    }
    info!("Sync phase: Network");
    update_status(&status_tx, SyncPhase::Network, upload_count, None, &hw, history.len());

    let nets = tokio::task::spawn_blocking(hardware::collect_network)
        .await
        .unwrap_or_default();
    hw.network_interfaces = nets;

    // First inventory upload — partial but almost complete (no software yet).
    let partial_upload = upload_inventory(
        &client, &base_url, &token, &device_id, &hw, &sw, "network",
    ).await;
    if partial_upload.is_ok() {
        upload_count += 1;
    }

    record_phase(&mut history, "network", &hw, None, &[]);
    info!("Phase Network complete — {} interfaces", hw.network_interfaces.len());

    // ── PHASE 6: Software ─────────────────────────────────────────────────
    if cancellable_sleep(phase_delay(PHASE_SOFTWARE_DELAY), &mut cancel_rx).await {
        return;
    }
    info!("Sync phase: Software");
    update_status(&status_tx, SyncPhase::Software, upload_count, None, &hw, history.len());

    sw = tokio::task::spawn_blocking(software::collect)
        .await
        .unwrap_or_else(|e| {
            warn!("Software collection panicked: {}", e);
            SoftwareList::default()
        });

    info!("Phase Software complete — {} apps", sw.apps.len());

    // Full upload — first complete snapshot.
    let now = chrono::Utc::now().to_rfc3339();
    match upload_inventory(&client, &base_url, &token, &device_id, &hw, &sw, "complete").await {
        Ok(()) => {
            upload_count += 1;
            update_status(&status_tx, SyncPhase::Incremental, upload_count, Some(&now), &hw, history.len());
            info!("Initial full sync complete (upload #{})", upload_count);
        }
        Err(e) => {
            warn!("Initial full sync upload failed: {}", e);
            update_status_err(&status_tx, SyncPhase::Incremental, upload_count, &e.to_string(), &hw, history.len());
        }
    }

    record_phase(&mut history, "complete", &hw, Some(&sw), &[]);
    save_history_quiet(&history);

    // ── INCREMENTAL MODE ──────────────────────────────────────────────────
    info!("Entering incremental sync mode (mode={:?})", mode);

    // Sync mode-specific intervals.
    let telemetry_iv = mode.telemetry_interval();
    let storage_iv = mode.storage_network_interval();
    let software_iv = mode.software_interval();
    let full_upload_iv = mode.full_upload_interval();

    // Track when each subsystem was last refreshed.
    let mut last_storage = Instant::now();
    let mut last_software = Instant::now();
    let mut last_full_upload = Instant::now();
    let mut last_history_save = Instant::now();

    // Previous snapshot for diff detection.
    let mut prev_hw = hw.clone();
    let mut prev_sw = sw.clone();

    loop {
        if cancellable_sleep(telemetry_iv, &mut cancel_rx).await {
            info!("Inventory collector cancelled during incremental mode");
            save_history_quiet(&history);
            return;
        }

        // Always refresh lightweight telemetry.
        let lw_result = {
            let mut sys_take = unsafe_take_sys(&mut sys);
            let res = tokio::task::spawn_blocking(move || {
                let m = hardware::collect_lightweight(&mut sys_take);
                (m, sys_take)
            })
            .await
            .unwrap_or_else(|e| {
                warn!("Lightweight telemetry panicked: {}", e);
                (hardware::LightweightMetrics::default(), System::new())
            });
            sys = res.1;
            res.0
        };

        hw.cpu.usage_percent = lw_result.cpu_usage_percent;
        hw.memory.used_bytes = lw_result.memory_used_bytes;
        hw.memory.total_bytes = lw_result.memory_total_bytes;
        hw.uptime_secs = lw_result.uptime_secs;

        // Refresh current memory snapshot for accurate data.
        hw.memory.available_bytes = hw.memory.total_bytes.saturating_sub(hw.memory.used_bytes);

        // Check if heavier subsystems need refresh.
        let mut changes: Vec<String> = Vec::new();

        if last_storage.elapsed() >= storage_iv {
            debug!("Incremental: refreshing storage + network");
            let new_disks = tokio::task::spawn_blocking(hardware::collect_storage)
                .await
                .unwrap_or_default();
            let new_nets = tokio::task::spawn_blocking(hardware::collect_network)
                .await
                .unwrap_or_default();

            // Detect changes before overwriting.
            let old_hw_snapshot = HardwareInfo {
                disks: prev_hw.disks.clone(),
                network_interfaces: prev_hw.network_interfaces.clone(),
                ..Default::default()
            };
            let new_hw_snapshot = HardwareInfo {
                disks: new_disks.clone(),
                network_interfaces: new_nets.clone(),
                ..Default::default()
            };
            changes.extend(diff::diff_hardware(&old_hw_snapshot, &new_hw_snapshot));

            hw.disks = new_disks;
            hw.network_interfaces = new_nets;
            last_storage = Instant::now();
        }

        if software_iv.as_secs() > 0 && last_software.elapsed() >= software_iv {
            debug!("Incremental: refreshing software list");
            let new_sw = tokio::task::spawn_blocking(software::collect)
                .await
                .unwrap_or_else(|e| {
                    warn!("Software collection panicked: {}", e);
                    SoftwareList::default()
                });
            changes.extend(diff::diff_software(&prev_sw, &new_sw));
            sw = new_sw;
            last_software = Instant::now();
        }

        // Detect CPU/RAM level changes vs previous.
        let hw_changes = diff::diff_hardware(&prev_hw, &hw);
        changes.extend(hw_changes);

        if !changes.is_empty() {
            info!("Incremental sync detected {} change(s): {:?}", changes.len(), changes);
            record_phase(&mut history, "incremental", &hw, None, &changes);
        }

        // Upload telemetry every tick.
        upload_telemetry_quiet(&client, &base_url, &token, &device_id, &lw_result).await;

        // Full upload periodically.
        if last_full_upload.elapsed() >= full_upload_iv {
            let now = chrono::Utc::now().to_rfc3339();
            match upload_inventory(&client, &base_url, &token, &device_id, &hw, &sw, "incremental").await {
                Ok(()) => {
                    upload_count += 1;
                    update_status(&status_tx, SyncPhase::Incremental, upload_count, Some(&now), &hw, history.len());
                    debug!("Incremental full upload ok (count={})", upload_count);
                }
                Err(e) => {
                    debug!("Incremental full upload failed (non-fatal): {}", e);
                    update_status_err(&status_tx, SyncPhase::Incremental, upload_count, &e.to_string(), &hw, history.len());
                }
            }
            last_full_upload = Instant::now();
        } else {
            // Update status with latest hw even without upload.
            let prev_status = status_tx.borrow().clone();
            let _ = status_tx.send(InventoryStatus {
                hardware: Some(hw.clone()),
                history_size: history.len(),
                ..prev_status
            });
        }

        // Persist history periodically.
        if last_history_save.elapsed() >= HISTORY_SAVE_INTERVAL {
            save_history_quiet(&history);
            last_history_save = Instant::now();
        }

        // Update previous snapshots for next diff cycle.
        prev_hw = hw.clone();
        prev_sw = sw.clone();
    }
}

// ---------------------------------------------------------------------------
//  Status update helpers
// ---------------------------------------------------------------------------

fn update_status(
    tx: &watch::Sender<InventoryStatus>,
    phase: SyncPhase,
    upload_count: u64,
    last_upload_at: Option<&str>,
    hw: &HardwareInfo,
    history_size: usize,
) {
    // Preserve existing sync_mode from the current status
    let prev_mode = tx.borrow().sync_mode.clone();
    let _ = tx.send(InventoryStatus {
        running: true,
        phase,
        last_upload_at: last_upload_at.map(|s| s.to_string()),
        upload_count,
        last_error: None,
        hardware: Some(hw.clone()),
        history_size,
        sync_mode: prev_mode,
    });
}

fn update_status_err(
    tx: &watch::Sender<InventoryStatus>,
    phase: SyncPhase,
    upload_count: u64,
    error: &str,
    hw: &HardwareInfo,
    history_size: usize,
) {
    let prev_mode = tx.borrow().sync_mode.clone();
    let _ = tx.send(InventoryStatus {
        running: true,
        phase,
        last_upload_at: None,
        upload_count,
        last_error: Some(error.to_string()),
        hardware: Some(hw.clone()),
        history_size,
        sync_mode: prev_mode,
    });
}

// ---------------------------------------------------------------------------
//  History helpers
// ---------------------------------------------------------------------------

fn record_phase(
    history: &mut SyncHistory,
    phase: &str,
    hw: &HardwareInfo,
    sw: Option<&SoftwareList>,
    changes: &[String],
) {
    history.append(SyncSnapshot {
        timestamp: chrono::Utc::now().to_rfc3339(),
        phase: phase.to_string(),
        hardware: Some(hw.clone()),
        software: sw.cloned(),
        changes: changes.to_vec(),
    });
}

fn save_history_quiet(history: &SyncHistory) {
    if let Err(e) = history.save() {
        warn!("Failed to save sync history: {}", e);
    }
}

// ---------------------------------------------------------------------------
//  HTTP upload helpers
// ---------------------------------------------------------------------------

async fn upload_inventory(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
    device_id: &str,
    hw: &HardwareInfo,
    sw: &SoftwareList,
    phase: &str,
) -> Result<()> {
    let payload = InventoryPayload {
        device_id: device_id.to_string(),
        hardware: hw.clone(),
        software: sw.clone(),
        collected_at: chrono::Utc::now().to_rfc3339(),
        sync_phase: phase.to_string(),
    };

    let mut req = client
        .post(format!("{}/api/bd/inventory", base_url))
        .json(&payload);

    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    } else {
        req = req.header("X-Device-Id", device_id);
    }

    let resp = req.send().await.context("Inventory upload HTTP failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Inventory upload HTTP {}: {}", status, text);
    }

    let body: ApiResponse = resp
        .json()
        .await
        .context("Failed to parse inventory response")?;

    if !body.success {
        anyhow::bail!(
            "Inventory upload rejected: {}",
            body.error.unwrap_or_else(|| "unknown".into())
        );
    }

    Ok(())
}

async fn upload_telemetry_quiet(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
    device_id: &str,
    metrics: &hardware::LightweightMetrics,
) {
    let payload = TelemetryPayload {
        device_id: device_id.to_string(),
        cpu_usage_percent: metrics.cpu_usage_percent,
        memory_used_bytes: metrics.memory_used_bytes,
        memory_total_bytes: metrics.memory_total_bytes,
        uptime_secs: metrics.uptime_secs,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };

    let mut req = client
        .post(format!("{}/api/bd/telemetry", base_url))
        .json(&payload);

    if !token.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", token));
    } else {
        req = req.header("X-Device-Id", device_id);
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            debug!("Telemetry uploaded");
        }
        Ok(resp) => {
            debug!("Telemetry upload HTTP {}", resp.status());
        }
        Err(e) => {
            debug!("Telemetry upload failed (non-fatal): {}", e);
        }
    }
}

// ---------------------------------------------------------------------------
//  System object shuttle (for spawn_blocking)
// ---------------------------------------------------------------------------

/// Move a System reference out so it can be sent into spawn_blocking.
///
/// Replaces the original with an empty System and returns the original.
/// Caller must put the returned System back into the mutable reference
/// after the blocking task completes.
fn unsafe_take_sys(sys: &mut System) -> System {
    std::mem::replace(sys, System::new())
}


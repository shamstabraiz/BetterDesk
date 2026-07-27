//! System metrics collection and heartbeat payload construction.
//!
//! Collects CPU, memory, disk, and network stats using `sysinfo`, and
//! maps them to CDAP widget values for the gateway.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use sysinfo::{Disks, Networks, System};

use super::protocol::SystemMetrics;
use super::CdapConfig;

static BOOT_TIME: AtomicU64 = AtomicU64::new(0);

/// Initialize boot time (called once at agent start).
pub fn init_boot_time() {
    let boot = System::boot_time();
    BOOT_TIME.store(boot, Ordering::Relaxed);
}

/// Collect current system metrics and build widget_values map.
pub fn collect_metrics(config: &CdapConfig) -> super::connection::HeartbeatPayload {
    let start = std::time::Instant::now();
    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();

    // CPU — global average
    let cpu = sys.global_cpu_usage() as f64;

    // Memory
    let total_mem = sys.total_memory();
    let used_mem = sys.used_memory();
    let memory = if total_mem > 0 {
        (used_mem as f64 / total_mem as f64) * 100.0
    } else {
        0.0
    };

    // Disk
    let disks = Disks::new_with_refreshed_list();
    let (total_disk, used_disk) = disks.iter().fold((0u64, 0u64), |(t, u), d| {
        (
            t + d.total_space(),
            u + (d.total_space() - d.available_space()),
        )
    });
    let disk = if total_disk > 0 {
        (used_disk as f64 / total_disk as f64) * 100.0
    } else {
        0.0
    };

    // Network throughput (delta-based, approximate per heartbeat)
    let networks = Networks::new_with_refreshed_list();
    let (tx_bytes, rx_bytes) = networks.iter().fold((0u64, 0u64), |(tx, rx), (_name, data)| {
        (tx + data.transmitted(), rx + data.received())
    });
    let tx_kbs = tx_bytes as f64 / 1024.0 / config.heartbeat_interval_secs.max(1) as f64;
    let rx_kbs = rx_bytes as f64 / 1024.0 / config.heartbeat_interval_secs.max(1) as f64;

    // Widget values
    let mut values = HashMap::new();
    values.insert("sys_cpu".into(), serde_json::json!(round2(cpu)));
    values.insert("sys_memory".into(), serde_json::json!(round2(memory)));
    values.insert("sys_disk".into(), serde_json::json!(round2(disk)));
    values.insert("sys_network_tx".into(), serde_json::json!(round2(tx_kbs)));
    values.insert("sys_network_rx".into(), serde_json::json!(round2(rx_kbs)));

    // Static info
    values.insert(
        "sys_hostname".into(),
        serde_json::json!(System::host_name().unwrap_or_default()),
    );
    values.insert(
        "sys_os".into(),
        serde_json::json!(format!(
            "{} {}",
            System::name().unwrap_or_default(),
            System::os_version().unwrap_or_default()
        )),
    );
    values.insert("sys_uptime".into(), serde_json::json!(format_uptime()));
    values.insert(
        "sys_total_ram".into(),
        serde_json::json!(format!("{:.1} GB", total_mem as f64 / 1_073_741_824.0)),
    );
    values.insert(
        "sys_total_disk".into(),
        serde_json::json!(format!("{:.1} GB", total_disk as f64 / 1_073_741_824.0)),
    );
    values.insert("sys_arch".into(), serde_json::json!(std::env::consts::ARCH));

    // IP address (first non-loopback)
    if let Some(ip) = get_primary_ip() {
        values.insert("sys_ip_address".into(), serde_json::json!(ip));
    }

    // Defender status (LED)
    values.insert(
        "defender_status".into(),
        serde_json::json!(if is_defender_running() {
            "green"
        } else {
            "red"
        }),
    );

    log::debug!("CDAP metrics collection took {:?}", start.elapsed());

    super::connection::HeartbeatPayload {
        metrics: SystemMetrics {
            cpu: round2(cpu),
            memory: round2(memory),
            disk: round2(disk),
        },
        widget_values: Some(values),
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn format_uptime() -> String {
    let boot = BOOT_TIME.load(Ordering::Relaxed);
    if boot == 0 {
        return "Unknown".into();
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let secs = now.saturating_sub(boot);
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{}d {}h {}m", d, h, m)
    } else {
        format!("{}h {}m", h, m)
    }
}

fn get_primary_ip() -> Option<String> {
    // Use a UDP socket trick to find the LAN IP
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    let addr = socket.local_addr().ok()?;
    Some(addr.ip().to_string())
}

fn is_defender_running() -> bool {
    // Check if MsMpEng.exe is running — cached for 5 minutes.
    // Uses targeted PID check instead of full process enumeration to avoid
    // freezing the system (ProcessesToUpdate::All scans ALL processes via WMI).
    use std::sync::Mutex;
    use std::time::Instant;

    static CACHE: Mutex<Option<(bool, Instant)>> = Mutex::new(None);
    const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

    if let Ok(guard) = CACHE.lock() {
        if let Some((val, ts)) = *guard {
            if ts.elapsed() < CACHE_TTL {
                return val;
            }
        }
    }

    // Use a lightweight approach: check if MsMpEng.exe is in the process
    // list via tasklist command (much cheaper than sysinfo full refresh).
    let running = is_process_running("MsMpEng.exe");

    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((running, Instant::now()));
    }

    running
}

/// Lightweight process existence check using tasklist (Windows) or pgrep (Unix).
/// Much cheaper than sysinfo::System::refresh_processes(All) which does deep WMI scan.
#[cfg(target_os = "windows")]
fn is_process_running(name: &str) -> bool {
    use std::process::Command;
    match Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {}", name), "/NH", "/FO", "CSV"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
    {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.contains(name)
        }
        Err(_) => false,
    }
}

#[cfg(not(target_os = "windows"))]
fn is_process_running(name: &str) -> bool {
    use std::process::Command;
    Command::new("pgrep")
        .args(["-x", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Lightweight fallback when `collect_metrics` panics inside `spawn_blocking`.
pub fn collect_metrics_fallback() -> super::connection::HeartbeatPayload {
    super::connection::HeartbeatPayload {
        metrics: SystemMetrics {
            cpu: 0.0,
            memory: 0.0,
            disk: 0.0,
        },
        widget_values: None,
    }
}

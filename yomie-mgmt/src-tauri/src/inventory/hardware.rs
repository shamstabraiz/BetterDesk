//! Hardware information collection via the `sysinfo` crate.
//!
//! Provides both a single-shot `collect()` for full snapshots and staged
//! collection functions for the phased sync architecture:
//!
//! - `collect_lightweight()` — CPU%, RAM%, uptime (no blocking, <1ms)
//! - `collect_basic()`       — hostname, OS, arch, core counts, total RAM
//! - `collect_cpu_detailed()` — brand, vendor, frequency (200ms measurement)
//! - `collect_storage()`     — disk enumeration
//! - `collect_network()`     — network interface enumeration

use serde::{Deserialize, Serialize};
use sysinfo::{CpuRefreshKind, Disks, MemoryRefreshKind, Networks, RefreshKind, System};

// ---------------------------------------------------------------------------
//  Public types
// ---------------------------------------------------------------------------

/// Complete hardware snapshot for a single device.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub architecture: String,
    pub uptime_secs: u64,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disks: Vec<DiskInfo>,
    pub network_interfaces: Vec<NetworkInterfaceInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuInfo {
    pub brand: String,
    pub vendor_id: String,
    pub physical_cores: usize,
    pub logical_cores: usize,
    /// Current average CPU usage percentage (0-100).
    pub usage_percent: f32,
    /// Per-core frequency in MHz.
    pub frequency_mhz: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryInfo {
    /// Total physical RAM in bytes.
    pub total_bytes: u64,
    /// Used RAM in bytes.
    pub used_bytes: u64,
    /// Available RAM in bytes.
    pub available_bytes: u64,
    /// Total swap in bytes.
    pub swap_total_bytes: u64,
    /// Used swap in bytes.
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub file_system: String,
    pub disk_type: String,
    /// Total space in bytes.
    pub total_bytes: u64,
    /// Available space in bytes.
    pub available_bytes: u64,
    pub is_removable: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub mac_address: String,
    /// IP addresses assigned (IPv4 and IPv6).
    pub ip_addresses: Vec<String>,
    /// Total bytes received since boot.
    pub rx_bytes: u64,
    /// Total bytes transmitted since boot.
    pub tx_bytes: u64,
}

/// Lightweight telemetry — only real-time metrics, no enumeration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LightweightMetrics {
    pub cpu_usage_percent: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub uptime_secs: u64,
}

/// Basic host metadata — fast, no measurement delay.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BasicHardwareInfo {
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub architecture: String,
    pub uptime_secs: u64,
    pub physical_cores: usize,
    pub logical_cores: usize,
    pub total_memory_bytes: u64,
    pub total_swap_bytes: u64,
}

// ---------------------------------------------------------------------------
//  Staged collection functions (for phased sync)
// ---------------------------------------------------------------------------

/// **Phase 1: Lightweight** — CPU usage %, RAM usage, uptime.
///
/// Near-instant (<1ms).  Uses a pre-existing `System` reference to avoid
/// re-creating the sysinfo object each time.  If `sys` is freshly created,
/// CPU usage will read 0%; call `refresh_cpu_all()` once beforehand.
pub fn collect_lightweight(sys: &mut System) -> LightweightMetrics {
    sys.refresh_memory_specifics(MemoryRefreshKind::everything());
    sys.refresh_cpu_all();

    let cpus = sys.cpus();
    let usage: f32 = if cpus.is_empty() {
        0.0
    } else {
        cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
    };

    LightweightMetrics {
        cpu_usage_percent: (usage * 100.0).round() / 100.0,
        memory_used_bytes: sys.used_memory(),
        memory_total_bytes: sys.total_memory(),
        uptime_secs: System::uptime(),
    }
}

/// **Phase 2: Basic hardware** — hostname, OS, arch, core counts, total RAM.
///
/// No blocking calls, no WMI queries. ~1ms.
pub fn collect_basic() -> BasicHardwareInfo {
    let sys = System::new_with_specifics(
        RefreshKind::new().with_memory(MemoryRefreshKind::new().with_ram()),
    );
    BasicHardwareInfo {
        hostname: System::host_name().unwrap_or_default(),
        os_name: System::name().unwrap_or_default(),
        os_version: System::os_version().unwrap_or_default(),
        kernel_version: System::kernel_version().unwrap_or_default(),
        architecture: std::env::consts::ARCH.to_string(),
        uptime_secs: System::uptime(),
        physical_cores: sys.physical_core_count().unwrap_or(0),
        logical_cores: sys.cpus().len().max(1),
        total_memory_bytes: sys.total_memory(),
        total_swap_bytes: sys.total_swap(),
    }
}

/// **Phase 3: CPU detailed** — brand, vendor, frequency with 200ms measurement.
///
/// Includes a 200ms sleep between two refresh calls so that CPU usage is
/// measured accurately.  Run on a blocking thread.
pub fn collect_cpu_detailed() -> CpuInfo {
    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_cpu(CpuRefreshKind::everything()),
    );
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_all();

    let cpus = sys.cpus();
    let brand = cpus.first().map(|c| c.brand().to_string()).unwrap_or_default();
    let vendor_id = cpus.first().map(|c| c.vendor_id().to_string()).unwrap_or_default();
    let freq = cpus.first().map(|c| c.frequency()).unwrap_or(0);
    let usage: f32 = if cpus.is_empty() {
        0.0
    } else {
        cpus.iter().map(|c| c.cpu_usage()).sum::<f32>() / cpus.len() as f32
    };

    CpuInfo {
        brand,
        vendor_id,
        physical_cores: sys.physical_core_count().unwrap_or(0),
        logical_cores: cpus.len(),
        usage_percent: (usage * 100.0).round() / 100.0,
        frequency_mhz: freq,
    }
}

/// **Phase 4: Storage** — enumerate all mounted disks.
pub fn collect_storage() -> Vec<DiskInfo> {
    let disks_obj = Disks::new_with_refreshed_list();
    disks_obj
        .iter()
        .map(|d| DiskInfo {
            name: d.name().to_string_lossy().to_string(),
            mount_point: d.mount_point().to_string_lossy().to_string(),
            file_system: String::from_utf8_lossy(d.file_system().as_encoded_bytes()).to_string(),
            disk_type: format!("{:?}", d.kind()),
            total_bytes: d.total_space(),
            available_bytes: d.available_space(),
            is_removable: d.is_removable(),
        })
        .collect()
}

/// **Phase 5: Network** — enumerate all network interfaces.
pub fn collect_network() -> Vec<NetworkInterfaceInfo> {
    let networks_obj = Networks::new_with_refreshed_list();
    networks_obj
        .iter()
        .map(|(name, data)| NetworkInterfaceInfo {
            name: name.clone(),
            mac_address: data.mac_address().to_string(),
            ip_addresses: data
                .ip_networks()
                .iter()
                .map(|ip| ip.addr.to_string())
                .collect(),
            rx_bytes: data.total_received(),
            tx_bytes: data.total_transmitted(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
//  Full collection (convenience — calls all stages)
// ---------------------------------------------------------------------------

/// Collect a full hardware snapshot of the local machine.
///
/// Runs on a blocking thread. Typical duration: 300-500ms (CPU measurement
/// accounts for the 200ms sleep).
pub fn collect() -> HardwareInfo {
    let start = std::time::Instant::now();

    let basic = collect_basic();
    let cpu = collect_cpu_detailed();

    let memory = {
        let sys = System::new_with_specifics(
            RefreshKind::new().with_memory(MemoryRefreshKind::everything()),
        );
        MemoryInfo {
            total_bytes: sys.total_memory(),
            used_bytes: sys.used_memory(),
            available_bytes: sys.available_memory(),
            swap_total_bytes: sys.total_swap(),
            swap_used_bytes: sys.used_swap(),
        }
    };

    let disks = collect_storage();
    let network_interfaces = collect_network();

    let info = HardwareInfo {
        hostname: basic.hostname,
        os_name: basic.os_name,
        os_version: basic.os_version,
        kernel_version: basic.kernel_version,
        architecture: basic.architecture,
        uptime_secs: basic.uptime_secs,
        cpu,
        memory,
        disks,
        network_interfaces,
    };
    log::debug!("Hardware: full collection took {:?}", start.elapsed());
    info
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_collect() {
        let info = collect();
        assert!(!info.hostname.is_empty(), "hostname should not be empty");
        assert!(info.cpu.logical_cores > 0, "should detect at least 1 core");
        assert!(info.memory.total_bytes > 0, "should detect RAM");
        // Disks and network interfaces may be empty in CI containers, so
        // we just verify the call succeeds without panic.
        println!("HW info: {}", serde_json::to_string_pretty(&info).unwrap());
    }
}

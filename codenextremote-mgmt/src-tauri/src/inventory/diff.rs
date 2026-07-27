//! Change detection between inventory snapshots.
//!
//! Compares two hardware/software snapshots and produces human-readable
//! descriptions of what changed.  Used by the phased collector to record
//! meaningful events in the sync history.

use super::hardware::{DiskInfo, HardwareInfo, NetworkInterfaceInfo};
use super::software::SoftwareList;

// ---------------------------------------------------------------------------
//  Public API
// ---------------------------------------------------------------------------

/// Compare two hardware snapshots and return a list of change descriptions.
pub fn diff_hardware(old: &HardwareInfo, new: &HardwareInfo) -> Vec<String> {
    let mut changes = Vec::new();

    // Hostname
    if old.hostname != new.hostname && !old.hostname.is_empty() {
        changes.push(format!(
            "Hostname changed: {} → {}",
            old.hostname, new.hostname
        ));
    }

    // OS version
    if old.os_version != new.os_version && !old.os_version.is_empty() {
        changes.push(format!(
            "OS version changed: {} → {}",
            old.os_version, new.os_version
        ));
    }

    // CPU usage spike (>30pp jump)
    let cpu_delta = (new.cpu.usage_percent - old.cpu.usage_percent).abs();
    if cpu_delta > 30.0 {
        changes.push(format!(
            "CPU usage jump: {:.0}% → {:.0}%",
            old.cpu.usage_percent, new.cpu.usage_percent
        ));
    }

    // Memory usage spike (>20% of total)
    if old.memory.total_bytes > 0 {
        let old_pct = (old.memory.used_bytes as f64) / (old.memory.total_bytes as f64) * 100.0;
        let new_pct = (new.memory.used_bytes as f64) / (new.memory.total_bytes as f64) * 100.0;
        if (new_pct - old_pct).abs() > 20.0 {
            changes.push(format!(
                "Memory usage jump: {:.0}% → {:.0}%",
                old_pct, new_pct
            ));
        }
    }

    // Disks: added, removed, space changed >5%
    diff_disks(&old.disks, &new.disks, &mut changes);

    // Network interfaces: added, removed, IP changed
    diff_networks(&old.network_interfaces, &new.network_interfaces, &mut changes);

    changes
}

/// Compare two software lists and return a list of change descriptions.
pub fn diff_software(old: &SoftwareList, new: &SoftwareList) -> Vec<String> {
    let mut changes = Vec::new();

    // Index old apps by name
    let old_map: std::collections::HashMap<&str, &str> = old
        .apps
        .iter()
        .map(|a| (a.name.as_str(), a.version.as_str()))
        .collect();
    let new_map: std::collections::HashMap<&str, &str> = new
        .apps
        .iter()
        .map(|a| (a.name.as_str(), a.version.as_str()))
        .collect();

    // Newly installed
    for (name, ver) in &new_map {
        if !old_map.contains_key(name) {
            changes.push(format!("Software installed: {} v{}", name, ver));
        }
    }

    // Uninstalled
    for (name, _ver) in &old_map {
        if !new_map.contains_key(name) {
            changes.push(format!("Software removed: {}", name));
        }
    }

    // Updated (same name, different version)
    for (name, new_ver) in &new_map {
        if let Some(old_ver) = old_map.get(name) {
            if old_ver != new_ver {
                changes.push(format!(
                    "Software updated: {} {} → {}",
                    name, old_ver, new_ver
                ));
            }
        }
    }

    changes
}

// ---------------------------------------------------------------------------
//  Disk diff
// ---------------------------------------------------------------------------

fn diff_disks(old: &[DiskInfo], new: &[DiskInfo], changes: &mut Vec<String>) {
    let old_map: std::collections::HashMap<&str, &DiskInfo> =
        old.iter().map(|d| (d.mount_point.as_str(), d)).collect();
    let new_map: std::collections::HashMap<&str, &DiskInfo> =
        new.iter().map(|d| (d.mount_point.as_str(), d)).collect();

    // New disks
    for (mp, disk) in &new_map {
        if !old_map.contains_key(mp) {
            changes.push(format!(
                "Disk added: {} ({}, {:.1} GB)",
                mp,
                disk.file_system,
                disk.total_bytes as f64 / 1_073_741_824.0
            ));
        }
    }

    // Removed disks
    for (mp, _) in &old_map {
        if !new_map.contains_key(mp) {
            changes.push(format!("Disk removed: {}", mp));
        }
    }

    // Space changes >5%
    for (mp, new_disk) in &new_map {
        if let Some(old_disk) = old_map.get(mp) {
            if old_disk.total_bytes > 0 {
                let old_free_pct =
                    (old_disk.available_bytes as f64) / (old_disk.total_bytes as f64) * 100.0;
                let new_free_pct =
                    (new_disk.available_bytes as f64) / (new_disk.total_bytes as f64) * 100.0;
                if (new_free_pct - old_free_pct).abs() > 5.0 {
                    changes.push(format!(
                        "Disk {} free space: {:.1}% → {:.1}%",
                        mp, old_free_pct, new_free_pct
                    ));
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
//  Network diff
// ---------------------------------------------------------------------------

fn diff_networks(
    old: &[NetworkInterfaceInfo],
    new: &[NetworkInterfaceInfo],
    changes: &mut Vec<String>,
) {
    let old_map: std::collections::HashMap<&str, &NetworkInterfaceInfo> =
        old.iter().map(|n| (n.name.as_str(), n)).collect();
    let new_map: std::collections::HashMap<&str, &NetworkInterfaceInfo> =
        new.iter().map(|n| (n.name.as_str(), n)).collect();

    // New interfaces
    for (name, iface) in &new_map {
        if !old_map.contains_key(name) {
            changes.push(format!(
                "Network interface added: {} ({})",
                name,
                iface.ip_addresses.join(", ")
            ));
        }
    }

    // Removed interfaces
    for (name, _) in &old_map {
        if !new_map.contains_key(name) {
            changes.push(format!("Network interface removed: {}", name));
        }
    }

    // IP address changes
    for (name, new_iface) in &new_map {
        if let Some(old_iface) = old_map.get(name) {
            let mut old_ips: Vec<&str> = old_iface.ip_addresses.iter().map(|s| s.as_str()).collect();
            let mut new_ips: Vec<&str> = new_iface.ip_addresses.iter().map(|s| s.as_str()).collect();
            old_ips.sort();
            new_ips.sort();
            if old_ips != new_ips {
                changes.push(format!(
                    "IP changed on {}: [{}] → [{}]",
                    name,
                    old_ips.join(", "),
                    new_ips.join(", ")
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::hardware::*;
    use crate::inventory::software::*;

    #[test]
    fn test_diff_hardware_no_changes() {
        let hw = HardwareInfo::default();
        let changes = diff_hardware(&hw, &hw);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_diff_disk_added() {
        let old = HardwareInfo::default();
        let mut new = HardwareInfo::default();
        new.disks.push(DiskInfo {
            mount_point: "D:\\".into(),
            file_system: "NTFS".into(),
            total_bytes: 500_000_000_000,
            ..Default::default()
        });
        let changes = diff_hardware(&old, &new);
        assert!(changes.iter().any(|c| c.contains("Disk added")));
    }

    #[test]
    fn test_diff_software_install_remove() {
        let old = SoftwareList {
            apps: vec![InstalledApp {
                name: "OldApp".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let new = SoftwareList {
            apps: vec![InstalledApp {
                name: "NewApp".into(),
                version: "1.0".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let changes = diff_software(&old, &new);
        assert!(changes.iter().any(|c| c.contains("installed: NewApp")));
        assert!(changes.iter().any(|c| c.contains("removed: OldApp")));
    }
}

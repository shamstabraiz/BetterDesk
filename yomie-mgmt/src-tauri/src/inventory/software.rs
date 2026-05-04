//! Installed software enumeration.
//!
//! On Windows reads the Uninstall registry keys.
//! On Linux reads dpkg / rpm databases.
//! On macOS scans /Applications and Homebrew.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
//  Public types
// ---------------------------------------------------------------------------

/// A single installed application entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstalledApp {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_date: String,
    /// Estimated size in bytes (0 if unknown).
    pub size_bytes: u64,
}

/// Complete list of installed software on this device.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SoftwareList {
    pub apps: Vec<InstalledApp>,
    pub collected_at: String,
}

// ---------------------------------------------------------------------------
//  Collection
// ---------------------------------------------------------------------------

/// Collect the list of installed software.
pub fn collect() -> SoftwareList {
    let apps = platform_collect();
    SoftwareList {
        apps,
        collected_at: chrono::Utc::now().to_rfc3339(),
    }
}

// ---------------------------------------------------------------------------
//  Windows implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn platform_collect() -> Vec<InstalledApp> {
    use std::process::Command;
    use std::time::{Duration, Instant};
    use wait_timeout::ChildExt;

    let start = Instant::now();

    // Use PowerShell to enumerate Uninstall keys — works even without
    // elevated permissions and avoids direct winreg dependency.
    let script = r#"
        $paths = @(
            'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
            'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
            'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
        )
        $apps = foreach ($p in $paths) {
            Get-ItemProperty $p -ErrorAction SilentlyContinue |
                Where-Object { $_.DisplayName } |
                Select-Object DisplayName, DisplayVersion, Publisher, InstallDate, EstimatedSize
        }
        $apps | Sort-Object DisplayName -Unique |
            ConvertTo-Json -Compress -Depth 2
    "#;

    // Spawn as child process so we can enforce a timeout.
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());

    // Hide the PowerShell console window on Windows.
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = match cmd.spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("Failed to spawn PowerShell for software enumeration: {}", e);
            return Vec::new();
        }
    };

    // Wait with a hard timeout — PowerShell registry scan can take 2-5
    // seconds on cold boot and occasionally hang indefinitely.
    const PS_TIMEOUT: Duration = Duration::from_secs(10);

    match child.wait_timeout(PS_TIMEOUT) {
        Ok(Some(status)) if status.success() => {
            let stdout = child.stdout.take().map(|mut s| {
                let mut buf = String::new();
                use std::io::Read;
                let _ = s.read_to_string(&mut buf);
                buf
            }).unwrap_or_default();
            log::debug!("PowerShell software scan completed in {:?}", start.elapsed());
            parse_powershell_json(&stdout)
        }
        Ok(Some(_status)) => {
            log::warn!("PowerShell software scan failed (non-zero exit) in {:?}", start.elapsed());
            Vec::new()
        }
        Ok(None) => {
            // Timeout — kill the process
            log::warn!("PowerShell software scan TIMED OUT after {:?} — killing process", PS_TIMEOUT);
            let _ = child.kill();
            let _ = child.wait(); // reap zombie
            Vec::new()
        }
        Err(e) => {
            log::warn!("Failed to wait for PowerShell: {} — killing process", e);
            let _ = child.kill();
            let _ = child.wait();
            Vec::new()
        }
    }
}

#[cfg(target_os = "windows")]
fn parse_powershell_json(json_str: &str) -> Vec<InstalledApp> {
    // PowerShell may return a single object (not array) if only one app matches.
    let trimmed = json_str.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct PsApp {
        display_name: Option<String>,
        display_version: Option<String>,
        publisher: Option<String>,
        install_date: Option<String>,
        estimated_size: Option<u64>,
    }

    // Try array first, then single object.
    if let Ok(apps) = serde_json::from_str::<Vec<PsApp>>(trimmed) {
        return apps
            .into_iter()
            .filter_map(|a| {
                let name = a.display_name?;
                if name.is_empty() {
                    return None;
                }
                Some(InstalledApp {
                    name,
                    version: a.display_version.unwrap_or_default(),
                    publisher: a.publisher.unwrap_or_default(),
                    install_date: a.install_date.unwrap_or_default(),
                    size_bytes: a.estimated_size.unwrap_or(0) * 1024, // KB → bytes
                })
            })
            .collect();
    }

    if let Ok(a) = serde_json::from_str::<PsApp>(trimmed) {
        if let Some(name) = a.display_name {
            if !name.is_empty() {
                return vec![InstalledApp {
                    name,
                    version: a.display_version.unwrap_or_default(),
                    publisher: a.publisher.unwrap_or_default(),
                    install_date: a.install_date.unwrap_or_default(),
                    size_bytes: a.estimated_size.unwrap_or(0) * 1024,
                }];
            }
        }
    }

    log::warn!("Could not parse PowerShell software list output");
    Vec::new()
}

// ---------------------------------------------------------------------------
//  Linux implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn platform_collect() -> Vec<InstalledApp> {
    use std::process::Command;

    let mut apps = Vec::new();

    // Try dpkg (Debian/Ubuntu)
    if let Ok(output) = Command::new("dpkg-query")
        .args(["-W", "-f", "${Package}\\t${Version}\\t${Installed-Size}\\n"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 2 {
                    apps.push(InstalledApp {
                        name: parts[0].to_string(),
                        version: parts[1].to_string(),
                        publisher: String::new(),
                        install_date: String::new(),
                        size_bytes: parts.get(2).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
                            * 1024,
                    });
                }
            }
            return apps;
        }
    }

    // Try rpm (RHEL/CentOS/Fedora)
    if let Ok(output) = Command::new("rpm")
        .args(["-qa", "--queryformat", "%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{VENDOR}\\t%{SIZE}\\n"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 2 {
                    apps.push(InstalledApp {
                        name: parts[0].to_string(),
                        version: parts[1].to_string(),
                        publisher: parts.get(2).unwrap_or(&"").to_string(),
                        install_date: String::new(),
                        size_bytes: parts.get(3).and_then(|s| s.parse::<u64>().ok()).unwrap_or(0),
                    });
                }
            }
        }
    }

    apps
}

// ---------------------------------------------------------------------------
//  macOS implementation
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn platform_collect() -> Vec<InstalledApp> {
    use std::process::Command;

    let mut apps = Vec::new();

    // Use system_profiler to list apps
    if let Ok(output) = Command::new("system_profiler")
        .args(["SPApplicationsDataType", "-json"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse the JSON output — simplified extraction
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&stdout) {
                if let Some(arr) = val
                    .get("SPApplicationsDataType")
                    .and_then(|v| v.as_array())
                {
                    for entry in arr {
                        let name = entry
                            .get("_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let version = entry
                            .get("version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        if !name.is_empty() {
                            apps.push(InstalledApp {
                                name,
                                version,
                                publisher: entry
                                    .get("obtained_from")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                install_date: String::new(),
                                size_bytes: 0,
                            });
                        }
                    }
                }
            }
        }
    }

    apps
}

// Fallback for unsupported platforms
#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn platform_collect() -> Vec<InstalledApp> {
    log::warn!("Software enumeration not supported on this platform");
    Vec::new()
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_software_collect() {
        let list = collect();
        println!(
            "Found {} installed apps at {}",
            list.apps.len(),
            list.collected_at
        );
        // On a dev machine there should be at least some software detected
        // (may be 0 in minimal CI containers — that's OK).
        for app in list.apps.iter().take(5) {
            println!("  - {} v{} ({})", app.name, app.version, app.publisher);
        }
    }
}

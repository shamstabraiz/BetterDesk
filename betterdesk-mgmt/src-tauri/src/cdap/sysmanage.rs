//! Windows system management — processes, services, power, environment.
//!
//! Uses PowerShell + Win32 API for deep system access.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sysinfo::{Pid, System};

// ---------------------------------------------------------------------------
//  Power actions
// ---------------------------------------------------------------------------

pub fn restart_system() -> Result<()> {
    run_cmd("shutdown", &["/r", "/t", "5", "/c", "Yomie remote restart"])?;
    Ok(())
}

pub fn shutdown_system() -> Result<()> {
    run_cmd("shutdown", &["/s", "/t", "5", "/c", "Yomie remote shutdown"])?;
    Ok(())
}

pub fn lock_screen() -> Result<()> {
    run_cmd("rundll32.exe", &["user32.dll,LockWorkStation"])?;
    Ok(())
}

pub fn logoff_user() -> Result<()> {
    run_cmd("shutdown", &["/l"])?;
    Ok(())
}

// ---------------------------------------------------------------------------
//  DNS / Temp
// ---------------------------------------------------------------------------

pub fn flush_dns() -> Result<String> {
    let output = run_cmd("ipconfig", &["/flushdns"])?;
    Ok(output)
}

pub fn clear_temp() -> Result<f64> {
    let output = run_powershell(
        "Get-ChildItem $env:TEMP -Recurse -Force -ErrorAction SilentlyContinue | \
         Remove-Item -Recurse -Force -ErrorAction SilentlyContinue; \
         (Get-ChildItem $env:TEMP -Recurse -Force -ErrorAction SilentlyContinue | \
         Measure-Object -Property Length -Sum).Sum / 1MB",
    )?;
    let remaining: f64 = output.trim().parse().unwrap_or(0.0);
    Ok(remaining)
}

// ---------------------------------------------------------------------------
//  Processes
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub memory_mb: f64,
    pub status: String,
}

pub fn list_processes() -> Result<Vec<ProcessInfo>> {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    // Second refresh for CPU delta
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut procs: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().to_string(),
            cpu: p.cpu_usage(),
            memory_mb: p.memory() as f64 / (1024.0 * 1024.0),
            status: format!("{:?}", p.status()),
        })
        .collect();

    procs.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));
    procs.truncate(200); // Top 200 by CPU
    Ok(procs)
}

pub fn kill_process(pid: u32) -> Result<()> {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    if let Some(process) = sys.process(Pid::from_u32(pid)) {
        process.kill();
        Ok(())
    } else {
        bail!("Process {} not found", pid)
    }
}

// ---------------------------------------------------------------------------
//  Services
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub name: String,
    pub display_name: String,
    pub status: String,
    pub start_type: String,
}

pub fn list_services() -> Result<Vec<ServiceInfo>> {
    let output = run_powershell(
        "Get-Service | Select-Object Name, DisplayName, Status, StartType | \
         ConvertTo-Json -Depth 2",
    )?;

    let raw: Vec<serde_json::Value> =
        serde_json::from_str(&output).context("Parse service list")?;

    let services = raw
        .into_iter()
        .map(|v| ServiceInfo {
            name: v
                .get("Name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            display_name: v
                .get("DisplayName")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            status: v
                .get("Status")
                .map(|v| match v.as_u64() {
                    Some(4) => "Running".into(),
                    Some(1) => "Stopped".into(),
                    _ => v.to_string(),
                })
                .unwrap_or_default(),
            start_type: v
                .get("StartType")
                .map(|v| match v.as_u64() {
                    Some(2) => "Automatic".into(),
                    Some(3) => "Manual".into(),
                    Some(4) => "Disabled".into(),
                    _ => v.to_string(),
                })
                .unwrap_or_default(),
        })
        .collect();

    Ok(services)
}

pub fn control_service(name: &str, command: &str) -> Result<()> {
    validate_service_name(name)?;
    match command {
        "start" => {
            run_powershell(&format!("Start-Service -Name '{}'", name))?;
        }
        "stop" => {
            run_powershell(&format!("Stop-Service -Name '{}' -Force", name))?;
        }
        "restart" => {
            run_powershell(&format!("Restart-Service -Name '{}' -Force", name))?;
        }
        "enable" => {
            run_powershell(&format!(
                "Set-Service -Name '{}' -StartupType Automatic",
                name
            ))?;
        }
        "disable" => {
            run_powershell(&format!(
                "Set-Service -Name '{}' -StartupType Disabled",
                name
            ))?;
        }
        _ => bail!("Unknown service command: {}", command),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
//  Installed software
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct SoftwareInfo {
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub install_date: String,
}

pub fn list_installed_software() -> Result<Vec<SoftwareInfo>> {
    let output = run_powershell(
        "Get-ItemProperty HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*, \
         HKLM:\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\* \
         -ErrorAction SilentlyContinue | \
         Where-Object { $_.DisplayName } | \
         Select-Object DisplayName, DisplayVersion, Publisher, InstallDate | \
         Sort-Object DisplayName | \
         ConvertTo-Json -Depth 2",
    )?;

    let raw: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    let software = raw
        .into_iter()
        .map(|v| SoftwareInfo {
            name: v
                .get("DisplayName")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            version: v
                .get("DisplayVersion")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            publisher: v
                .get("Publisher")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            install_date: v
                .get("InstallDate")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
        })
        .collect();

    Ok(software)
}

// ---------------------------------------------------------------------------
//  Startup programs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct StartupProgram {
    pub name: String,
    pub command: String,
    pub location: String,
}

pub fn list_startup_programs() -> Result<Vec<StartupProgram>> {
    let output = run_powershell(
        "Get-CimInstance Win32_StartupCommand | \
         Select-Object Name, Command, Location | \
         ConvertTo-Json -Depth 2",
    )?;

    let raw: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    let programs = raw
        .into_iter()
        .map(|v| StartupProgram {
            name: v
                .get("Name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            command: v
                .get("Command")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            location: v
                .get("Location")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
        })
        .collect();

    Ok(programs)
}

// ---------------------------------------------------------------------------
//  Environment variables
// ---------------------------------------------------------------------------

pub fn get_environment_variables() -> Result<Vec<(String, String)>> {
    let vars: Vec<(String, String)> = std::env::vars().collect();
    Ok(vars)
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

/// Run a system command and return stdout.
fn run_cmd(program: &str, args: &[&str]) -> Result<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .context(format!("Failed to run {}", program))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{} failed: {}", program, stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a PowerShell command and return stdout.
fn run_powershell(script: &str) -> Result<String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()
        .context("Failed to run PowerShell")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("PowerShell failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Validate service name against injection.
fn validate_service_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 256 {
        bail!("Invalid service name length");
    }
    // Allow only alphanumeric, hyphens, underscores, dots, spaces
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ' ')
    {
        bail!("Service name contains invalid characters");
    }
    Ok(())
}

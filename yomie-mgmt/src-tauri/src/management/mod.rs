//! Device management commands — lock screen, log out, restart, shutdown,
//! and a safe whitelist-based command execution facility.
//!
//! All mutating commands require `--admin` elevation on the caller side.
//! The Tauri command layer checks `service::is_elevated()` before dispatching.

use anyhow::{bail, Result};
use log::info;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
//  Types
// ---------------------------------------------------------------------------

/// Snapshot of device state returned to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub hostname: String,
    pub current_user: String,
    pub os: String,
    pub uptime_secs: u64,
    pub cpu_usage: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub disk_used_gb: f32,
    pub disk_total_gb: f32,
}

/// Result of a management command.
#[derive(Debug, Clone, Serialize)]
pub struct CommandResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Safe predefined commands that can be executed by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PredefinedCommand {
    GetProcessList,
    GetServicesStatus,
    ClearTemp,
    FlushDns,
    GetEventLog,
    PingGateway,
    GetDiskUsage,
    GetNetworkInfo,
}

// ---------------------------------------------------------------------------
//  System info
// ---------------------------------------------------------------------------

/// Collect a snapshot of device state.
pub fn get_device_info() -> DeviceInfo {
    use sysinfo::{Disks, System};

    let mut sys = System::new_all();
    sys.refresh_all();

    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());

    let current_user = whoami::username();

    let os = format!(
        "{} {} ({})",
        System::name().unwrap_or_default(),
        System::os_version().unwrap_or_default(),
        System::kernel_version().unwrap_or_default()
    );

    let uptime_secs = System::uptime();

    let cpu_usage = sys.global_cpu_usage();

    let ram_total_mb = sys.total_memory() / 1_048_576;
    let ram_used_mb = sys.used_memory() / 1_048_576;

    let disks = Disks::new_with_refreshed_list();
    let (disk_total, disk_used) = disks.iter().fold((0u64, 0u64), |(t, u), d| {
        (t + d.total_space(), u + d.total_space() - d.available_space())
    });

    DeviceInfo {
        hostname,
        current_user,
        os,
        uptime_secs,
        cpu_usage,
        ram_total_mb,
        ram_used_mb,
        disk_total_gb: disk_total as f32 / 1_073_741_824.0,
        disk_used_gb: disk_used as f32 / 1_073_741_824.0,
    }
}

// ---------------------------------------------------------------------------
//  Screen / session management
// ---------------------------------------------------------------------------

/// Lock the workstation screen.
pub fn lock_screen() -> Result<()> {
    info!("Management: locking screen");
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Shutdown::LockWorkStation;
        unsafe {
            if LockWorkStation() == 0 {
                bail!("LockWorkStation() failed");
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("loginctl").arg("lock-session").status()?;
    }
    Ok(())
}

/// Sign out the current user.
pub fn logoff_user() -> Result<()> {
    info!("Management: logging off user");
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("shutdown").args(["/l"]).status()?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("loginctl").arg("terminate-session").arg("self").status()?;
    }
    Ok(())
}

/// Request a system restart (30-second delay to allow user to save work).
pub fn restart_system(delay_secs: u32) -> Result<()> {
    info!("Management: requesting restart in {}s", delay_secs);
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("shutdown")
            .args(["/r", "/t", &delay_secs.to_string(), "/c", "Yomie remote restart"])
            .status()?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("shutdown")
            .args(["-r", &format!("+{}", delay_secs / 60)])
            .status()?;
    }
    Ok(())
}

/// Request a system shutdown.
pub fn shutdown_system(delay_secs: u32) -> Result<()> {
    info!("Management: requesting shutdown in {}s", delay_secs);
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("shutdown")
            .args(["/s", "/t", &delay_secs.to_string(), "/c", "Yomie remote shutdown"])
            .status()?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("shutdown")
            .args(["-h", &format!("+{}", delay_secs / 60)])
            .status()?;
    }
    Ok(())
}

/// Cancel a pending restart or shutdown.
pub fn abort_shutdown() -> Result<()> {
    info!("Management: aborting pending shutdown");
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("shutdown").args(["/a"]).status()?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::process::Command::new("shutdown").arg("-c").status()?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
//  Predefined safe commands
// ---------------------------------------------------------------------------

/// Execute one of the whitelisted predefined commands and return the output.
pub fn run_predefined(cmd: PredefinedCommand) -> Result<CommandResult> {
    info!("Management: running predefined command {:?}", cmd);

    let (program, args): (&str, Vec<&str>) = match cmd {
        PredefinedCommand::GetProcessList => {
            #[cfg(target_os = "windows")]
            { ("tasklist", vec!["/FO", "CSV"]) }
            #[cfg(not(target_os = "windows"))]
            { ("ps", vec!["aux"]) }
        }
        PredefinedCommand::GetServicesStatus => {
            #[cfg(target_os = "windows")]
            { ("sc", vec!["query", "type=", "all"]) }
            #[cfg(not(target_os = "windows"))]
            { ("systemctl", vec!["list-units", "--type=service", "--no-pager"]) }
        }
        PredefinedCommand::ClearTemp => {
            #[cfg(target_os = "windows")]
            {
                // Remove files from %TEMP%, ignore errors
                let temp = std::env::var("TEMP").unwrap_or_else(|_| "C:\\Windows\\Temp".into());
                return exec_and_capture("cmd", &["/C", &format!("del /Q /F /S \"{}\\*\"", temp)]);
            }
            #[cfg(not(target_os = "windows"))]
            { ("sh", vec!["-c", "rm -rf /tmp/* 2>/dev/null; echo done"]) }
        }
        PredefinedCommand::FlushDns => {
            #[cfg(target_os = "windows")]
            { ("ipconfig", vec!["/flushdns"]) }
            #[cfg(not(target_os = "windows"))]
            { ("resolvectl", vec!["flush-caches"]) }
        }
        PredefinedCommand::GetEventLog => {
            #[cfg(target_os = "windows")]
            { ("wevtutil", vec!["qe", "System", "/c:20", "/rd:true", "/f:text"]) }
            #[cfg(not(target_os = "windows"))]
            { ("journalctl", vec!["-n", "50", "--no-pager"]) }
        }
        PredefinedCommand::PingGateway => {
            #[cfg(target_os = "windows")]
            { ("ping", vec!["-n", "4", "8.8.8.8"]) }
            #[cfg(not(target_os = "windows"))]
            { ("ping", vec!["-c", "4", "8.8.8.8"]) }
        }
        PredefinedCommand::GetDiskUsage => {
            #[cfg(target_os = "windows")]
            { ("wmic", vec!["logicaldisk", "get", "size,freespace,caption"]) }
            #[cfg(not(target_os = "windows"))]
            { ("df", vec!["-h"]) }
        }
        PredefinedCommand::GetNetworkInfo => {
            #[cfg(target_os = "windows")]
            { ("ipconfig", vec!["/all"]) }
            #[cfg(not(target_os = "windows"))]
            { ("ip", vec!["addr", "show"]) }
        }
    };

    exec_and_capture(program, &args)
}

fn exec_and_capture(program: &str, args: &[&str]) -> Result<CommandResult> {
    let output = std::process::Command::new(program).args(args).output()?;
    Ok(CommandResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

//! Windows security module — Defender, firewall, event log, user accounts.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
//  Windows Defender
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct DefenderStatus {
    pub real_time_protection: bool,
    pub antivirus_enabled: bool,
    pub last_scan: String,
    pub signature_version: String,
    pub signature_age: u32,
}

pub fn get_defender_status() -> Result<DefenderStatus> {
    let output = run_powershell(
        "Get-MpComputerStatus | Select-Object \
         RealTimeProtectionEnabled, AntivirusEnabled, \
         FullScanEndTime, AntivirusSignatureVersion, \
         AntivirusSignatureAge | ConvertTo-Json",
    )?;

    let v: serde_json::Value = serde_json::from_str(&output).context("Parse Defender status")?;

    Ok(DefenderStatus {
        real_time_protection: v
            .get("RealTimeProtectionEnabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        antivirus_enabled: v
            .get("AntivirusEnabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        last_scan: v
            .get("FullScanEndTime")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .into(),
        signature_version: v
            .get("AntivirusSignatureVersion")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .into(),
        signature_age: v
            .get("AntivirusSignatureAge")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    })
}

pub fn start_defender_scan(scan_type: &str) -> Result<()> {
    let cmd = match scan_type {
        "quick" => "Start-MpScan -ScanType QuickScan",
        "full" => "Start-MpScan -ScanType FullScan",
        _ => bail!("Invalid scan type: {} (use 'quick' or 'full')", scan_type),
    };
    // Run async — scan takes a long time
    run_powershell_detached(cmd)?;
    Ok(())
}

// ---------------------------------------------------------------------------
//  Firewall
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct FirewallRule {
    pub name: String,
    pub direction: String,
    pub action: String,
    pub enabled: bool,
    pub profile: String,
    pub protocol: String,
    pub local_port: String,
}

pub fn list_firewall_rules() -> Result<Vec<FirewallRule>> {
    let output = run_powershell(
        "Get-NetFirewallRule | Select-Object -First 200 \
         DisplayName, Direction, Action, Enabled, Profile | \
         ConvertTo-Json -Depth 2",
    )?;

    let raw: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    let rules = raw
        .into_iter()
        .map(|v| FirewallRule {
            name: v
                .get("DisplayName")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            direction: match v.get("Direction").and_then(|v| v.as_u64()) {
                Some(1) => "Inbound".into(),
                Some(2) => "Outbound".into(),
                _ => "Unknown".into(),
            },
            action: match v.get("Action").and_then(|v| v.as_u64()) {
                Some(2) => "Allow".into(),
                Some(4) => "Block".into(),
                _ => "Unknown".into(),
            },
            enabled: v
                .get("Enabled")
                .and_then(|v| v.as_u64())
                .map(|v| v == 1)
                .unwrap_or(false),
            profile: v
                .get("Profile")
                .map(|v| v.to_string())
                .unwrap_or_default(),
            protocol: String::new(),
            local_port: String::new(),
        })
        .collect();

    Ok(rules)
}

pub fn manage_firewall_rule(command: &str, params: &serde_json::Value) -> Result<()> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    validate_safe_string(name)?;

    match command {
        "enable" => {
            run_powershell(&format!(
                "Enable-NetFirewallRule -DisplayName '{}'",
                name
            ))?;
        }
        "disable" => {
            run_powershell(&format!(
                "Disable-NetFirewallRule -DisplayName '{}'",
                name
            ))?;
        }
        "remove" => {
            run_powershell(&format!(
                "Remove-NetFirewallRule -DisplayName '{}'",
                name
            ))?;
        }
        "add" => {
            let direction = params
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("Inbound");
            let action = params
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("Block");
            let protocol = params
                .get("protocol")
                .and_then(|v| v.as_str())
                .unwrap_or("TCP");
            let port = params
                .get("port")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            validate_safe_string(direction)?;
            validate_safe_string(action)?;
            validate_safe_string(protocol)?;
            validate_safe_string(port)?;

            let mut cmd = format!(
                "New-NetFirewallRule -DisplayName '{}' -Direction {} -Action {}",
                name, direction, action
            );
            if !port.is_empty() {
                cmd.push_str(&format!(" -Protocol {} -LocalPort {}", protocol, port));
            }
            run_powershell(&cmd)?;
        }
        _ => bail!("Unknown firewall command: {}", command),
    }

    Ok(())
}

// ---------------------------------------------------------------------------
//  Event Log
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct EventLogEntry {
    pub time: String,
    pub level: String,
    pub source: String,
    pub message: String,
    pub event_id: u32,
}

pub fn get_event_log(log_name: &str, count: u32) -> Result<Vec<EventLogEntry>> {
    // Allow only well-known log names to prevent injection
    let safe_log = match log_name {
        "System" | "Application" | "Security" | "Setup" => log_name,
        _ => "System",
    };

    let count = count.min(500); // Cap at 500 entries

    let output = run_powershell(&format!(
        "Get-WinEvent -LogName '{}' -MaxEvents {} -ErrorAction SilentlyContinue | \
         Select-Object TimeCreated, LevelDisplayName, ProviderName, Message, Id | \
         ConvertTo-Json -Depth 2",
        safe_log, count
    ))?;

    let raw: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    let entries = raw
        .into_iter()
        .map(|v| EventLogEntry {
            time: v
                .get("TimeCreated")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            level: v
                .get("LevelDisplayName")
                .and_then(|v| v.as_str())
                .unwrap_or("Info")
                .into(),
            source: v
                .get("ProviderName")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            message: v
                .get("Message")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .chars()
                .take(500) // Truncate long messages
                .collect(),
            event_id: v.get("Id").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        })
        .collect();

    Ok(entries)
}

// ---------------------------------------------------------------------------
//  User Accounts
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct UserAccount {
    pub name: String,
    pub full_name: String,
    pub active: bool,
    pub admin: bool,
    pub last_logon: String,
}

pub fn list_user_accounts() -> Result<Vec<UserAccount>> {
    let output = run_powershell(
        "Get-LocalUser | Select-Object Name, FullName, Enabled, \
         LastLogon | ConvertTo-Json -Depth 2",
    )?;

    let raw: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();

    // Get admin group members
    let admin_output = run_powershell(
        "Get-LocalGroupMember -Group 'Administrators' -ErrorAction SilentlyContinue | \
         Select-Object -ExpandProperty Name | ConvertTo-Json",
    )
    .unwrap_or_default();

    let admin_names: Vec<String> = serde_json::from_str(&admin_output).unwrap_or_default();

    let accounts = raw
        .into_iter()
        .map(|v| {
            let name = v
                .get("Name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let is_admin = admin_names
                .iter()
                .any(|a| a.ends_with(&format!("\\{}", name)));

            UserAccount {
                name: name.clone(),
                full_name: v
                    .get("FullName")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .into(),
                active: v
                    .get("Enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                admin: is_admin,
                last_logon: v
                    .get("LastLogon")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Never")
                    .into(),
            }
        })
        .collect();

    Ok(accounts)
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

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

fn run_powershell_detached(script: &str) -> Result<()> {
    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .spawn()
        .context("Failed to spawn detached PowerShell")?;
    Ok(())
}

/// Validate a string for safe use in PowerShell commands (no injection).
fn validate_safe_string(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 512 {
        bail!("String too long or empty");
    }
    // Block dangerous chars that could escape PowerShell quotes
    if s.contains('\'')
        || s.contains('"')
        || s.contains('`')
        || s.contains('$')
        || s.contains(';')
        || s.contains('|')
        || s.contains('&')
        || s.contains('\n')
        || s.contains('\r')
    {
        bail!("String contains potentially dangerous characters");
    }
    Ok(())
}

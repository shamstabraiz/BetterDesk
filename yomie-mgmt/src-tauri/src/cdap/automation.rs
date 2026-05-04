//! Automation engine — script execution, scheduled tasks.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
//  Script Execution
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ScriptResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Execute a script with the specified engine. Returns stdout/stderr/exit_code.
///
/// Supported engines: `powershell`, `cmd`, `python`.
/// Script content is passed via stdin to prevent argument-injection.
pub fn execute_script(engine: &str, script: &str) -> Result<ScriptResult> {
    if script.len() > 64 * 1024 {
        bail!("Script too large (max 64 KiB)");
    }

    let start = std::time::Instant::now();

    let (program, args): (&str, Vec<&str>) = match engine {
        "powershell" | "ps1" => (
            "powershell",
            vec![
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "-", // read from stdin
            ],
        ),
        "cmd" | "bat" => ("cmd", vec!["/C", script]),
        "python" | "py" => ("python", vec!["-c", script]),
        _ => bail!("Unsupported script engine: {}", engine),
    };

    let mut child = std::process::Command::new(program)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .context("Failed to spawn script process")?;

    // For powershell and python, feed script via stdin
    if matches!(engine, "powershell" | "ps1" | "python" | "py") {
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(script.as_bytes());
            // stdin is dropped here, closing it
        }
    }

    let output = child
        .wait_with_output()
        .context("Failed to collect script output")?;

    let duration_ms = start.elapsed().as_millis() as u64;

    // Truncate output to prevent memory issues
    const MAX_OUTPUT: usize = 256 * 1024; // 256 KiB
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    Ok(ScriptResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout: stdout.chars().take(MAX_OUTPUT).collect(),
        stderr: stderr.chars().take(MAX_OUTPUT).collect(),
        duration_ms,
    })
}

// ---------------------------------------------------------------------------
//  Scheduled Tasks
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct ScheduledTask {
    pub name: String,
    pub path: String,
    pub state: String,
    pub last_run: String,
    pub next_run: String,
}

pub fn list_scheduled_tasks() -> Result<Vec<ScheduledTask>> {
    let output = run_powershell(
        "Get-ScheduledTask | Where-Object { $_.TaskPath -notlike '\\Microsoft\\*' } | \
         Select-Object -First 200 TaskName, TaskPath, State | \
         ForEach-Object { \
           $info = Get-ScheduledTaskInfo -TaskName $_.TaskName -TaskPath $_.TaskPath -ErrorAction SilentlyContinue; \
           [PSCustomObject]@{ \
             TaskName  = $_.TaskName; \
             TaskPath  = $_.TaskPath; \
             State     = $_.State.ToString(); \
             LastRun   = if($info) { $info.LastRunTime.ToString('o') } else { 'N/A' }; \
             NextRun   = if($info) { $info.NextRunTime.ToString('o') } else { 'N/A' }; \
           } \
         } | ConvertTo-Json -Depth 2",
    )?;

    let raw: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap_or_default();
    let tasks = raw
        .into_iter()
        .map(|v| ScheduledTask {
            name: v
                .get("TaskName")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            path: v
                .get("TaskPath")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .into(),
            state: v
                .get("State")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown")
                .into(),
            last_run: v
                .get("LastRun")
                .and_then(|v| v.as_str())
                .unwrap_or("N/A")
                .into(),
            next_run: v
                .get("NextRun")
                .and_then(|v| v.as_str())
                .unwrap_or("N/A")
                .into(),
        })
        .collect();

    Ok(tasks)
}

pub fn manage_scheduled_task(name: &str, command: &str) -> Result<String> {
    validate_task_name(name)?;

    let script = match command {
        "run" => format!("Start-ScheduledTask -TaskName '{}'", name),
        "enable" => format!("Enable-ScheduledTask -TaskName '{}'", name),
        "disable" => format!("Disable-ScheduledTask -TaskName '{}'", name),
        _ => bail!("Unknown task command: {} (use run/enable/disable)", command),
    };

    run_powershell(&script)?;
    Ok(format!("Task '{}': {} OK", name, command))
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

/// Validate a scheduled-task name to prevent injection.
fn validate_task_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 256 {
        bail!("Task name too long or empty");
    }
    // Task names may contain spaces but not shell metacharacters
    for c in name.chars() {
        if matches!(c, '\'' | '"' | '`' | '$' | ';' | '|' | '&' | '\n' | '\r') {
            bail!("Task name contains forbidden characters");
        }
    }
    Ok(())
}

/// OS-specific creation flag to hide console window.
trait CreationFlags {
    fn creation_flags(&mut self, flags: u32) -> &mut Self;
}

#[cfg(windows)]
impl CreationFlags for std::process::Command {
    fn creation_flags(&mut self, flags: u32) -> &mut Self {
        std::os::windows::process::CommandExt::creation_flags(self, flags)
    }
}

#[cfg(not(windows))]
impl CreationFlags for std::process::Command {
    fn creation_flags(&mut self, _flags: u32) -> &mut Self {
        self
    }
}

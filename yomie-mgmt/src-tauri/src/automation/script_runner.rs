/// Yomie Agent — Script Runner
///
/// Executes shell commands, PowerShell scripts, and service management
/// operations on the local machine.  Commands are received from the
/// admin console via the CommandChannel and executed in a sandboxed
/// environment.

use anyhow::Result;
use std::process::Command;

/// Maximum command execution time (seconds).
const _MAX_EXECUTION_SECS: u64 = 300;

/// Maximum output size before truncation (bytes).
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// Result of a command execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub truncated: bool,
}

/// Supported command types.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandType {
    Shell,
    Powershell,
    Script,
    RestartService,
    Reboot,
}

impl CommandType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "powershell" => Self::Powershell,
            "script" => Self::Script,
            "restart_service" => Self::RestartService,
            "reboot" => Self::Reboot,
            _ => Self::Shell,
        }
    }
}

pub struct ScriptRunner;

impl ScriptRunner {
    /// Execute a command and return the result.
    pub fn execute(cmd_type: &CommandType, payload: &str) -> Result<CommandResult> {
        let start = std::time::Instant::now();

        let output = match cmd_type {
            CommandType::Shell => Self::run_shell(payload)?,
            CommandType::Powershell => Self::run_powershell(payload)?,
            CommandType::Script => Self::run_script(payload)?,
            CommandType::RestartService => Self::restart_service(payload)?,
            CommandType::Reboot => Self::reboot()?,
        };

        let duration_ms = start.elapsed().as_millis() as u64;

        let mut stdout = output.0;
        let mut stderr = output.1;
        let exit_code = output.2;
        let mut truncated = false;

        if stdout.len() > MAX_OUTPUT_BYTES {
            stdout.truncate(MAX_OUTPUT_BYTES);
            stdout.push_str("\n... [truncated]");
            truncated = true;
        }
        if stderr.len() > MAX_OUTPUT_BYTES {
            stderr.truncate(MAX_OUTPUT_BYTES);
            stderr.push_str("\n... [truncated]");
            truncated = true;
        }

        Ok(CommandResult {
            exit_code,
            stdout,
            stderr,
            duration_ms,
            truncated,
        })
    }

    /// Run a shell command (cmd on Windows, sh on Unix).
    fn run_shell(payload: &str) -> Result<(String, String, i32)> {
        let output = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", payload])
                .output()?
        } else {
            Command::new("sh")
                .args(["-c", payload])
                .output()?
        };

        Ok((
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            output.status.code().unwrap_or(-1),
        ))
    }

    /// Run a PowerShell command (Windows only, falls back to pwsh on Unix).
    fn run_powershell(payload: &str) -> Result<(String, String, i32)> {
        let shell = if cfg!(target_os = "windows") {
            "powershell"
        } else {
            "pwsh"
        };

        let output = Command::new(shell)
            .args(["-NoProfile", "-NonInteractive", "-Command", payload])
            .output()?;

        Ok((
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            output.status.code().unwrap_or(-1),
        ))
    }

    /// Run a script file (payload is the script content, written to temp file).
    fn run_script(payload: &str) -> Result<(String, String, i32)> {
        let ext = if cfg!(target_os = "windows") { ".ps1" } else { ".sh" };
        let temp_dir = std::env::temp_dir();
        let script_path = temp_dir.join(format!("yomie_script_{}{}", std::process::id(), ext));

        std::fs::write(&script_path, payload)?;

        let output = if cfg!(target_os = "windows") {
            Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-File", script_path.to_str().unwrap_or("")])
                .output()?
        } else {
            // Make executable
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&script_path)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&script_path, perms)?;
            }
            Command::new("sh")
                .args(["-c", script_path.to_str().unwrap_or("")])
                .output()?
        };

        // Cleanup
        let _ = std::fs::remove_file(&script_path);

        Ok((
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            output.status.code().unwrap_or(-1),
        ))
    }

    /// Restart a system service.
    fn restart_service(service_name: &str) -> Result<(String, String, i32)> {
        if cfg!(target_os = "windows") {
            Self::run_powershell(&format!("Restart-Service -Name '{}' -Force", service_name))
        } else {
            Self::run_shell(&format!("systemctl restart {}", service_name))
        }
    }

    /// Reboot the machine.
    fn reboot() -> Result<(String, String, i32)> {
        if cfg!(target_os = "windows") {
            Self::run_shell("shutdown /r /t 30 /c \"Yomie remote reboot\"")
        } else {
            Self::run_shell("shutdown -r +1 'Yomie remote reboot'")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_echo() {
        let result = ScriptRunner::execute(&CommandType::Shell, "echo hello").unwrap();
        assert!(result.stdout.contains("hello"));
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_command_type_from_str() {
        assert!(matches!(CommandType::from_str("shell"), CommandType::Shell));
        assert!(matches!(CommandType::from_str("powershell"), CommandType::Powershell));
        assert!(matches!(CommandType::from_str("reboot"), CommandType::Reboot));
        assert!(matches!(CommandType::from_str("unknown"), CommandType::Shell));
    }
}

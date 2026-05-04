//! Elevated privilege service — run as Windows Service or Linux root daemon.
//!
//! When Yomie needs to:
//! - Change server configuration (writes to protected config directory)
//! - Capture the secure desktop (UAC prompts, login screen)
//! - Simulate input on elevated applications
//! - Access protected clipboard content
//! - Add firewall rules
//!
//! It can either run as a system service or elevate on demand via UAC.

use anyhow::Result;
use log::{info, warn};

/// Check if the current process has elevated (admin/root) privileges.
pub fn is_elevated() -> bool {
    #[cfg(target_os = "windows")]
    {
        is_elevated_windows()
    }
    #[cfg(target_os = "linux")]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(target_os = "macos")]
    {
        unsafe { libc::geteuid() == 0 }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

#[cfg(target_os = "windows")]
fn is_elevated_windows() -> bool {
    use std::mem;
    use std::ptr;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = ptr::null_mut();
        let process = GetCurrentProcess();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut size = mem::size_of::<TOKEN_ELEVATION>() as u32;

        let result = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            size,
            &mut size,
        );

        CloseHandle(token);

        result != 0 && elevation.TokenIsElevated != 0
    }
}

/// Apply a configuration change via an elevated helper process.
///
/// On Windows: spawns the same binary with `--apply-config <base64>` using
/// ShellExecuteW "runas", which triggers a UAC prompt. The elevated process
/// writes the config file and exits immediately.
///
/// On Linux/macOS: uses pkexec for graphical elevation.
pub fn apply_config_elevated(config_json: &str) -> Result<()> {
    if is_elevated() {
        // Already elevated — write directly
        let settings: crate::config::Settings = serde_json::from_str(config_json)?;
        settings.save()?;
        info!("Config written directly (already elevated)");
        return Ok(());
    }

    let encoded = base64_encode(config_json);
    let exe = std::env::current_exe()?;

    #[cfg(target_os = "windows")]
    {
        apply_config_elevated_windows(&exe, &encoded)?;
    }

    #[cfg(target_os = "linux")]
    {
        apply_config_elevated_linux(&exe, &encoded)?;
    }

    #[cfg(target_os = "macos")]
    {
        apply_config_elevated_linux(&exe, &encoded)?; // pkexec works on macOS too
    }

    Ok(())
}

/// Handle `--apply-config <base64>` CLI argument in the elevated process.
///
/// Called from `main.rs` before Tauri starts. Writes the config and exits.
pub fn handle_apply_config_cli() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--apply-config") {
        if let Some(encoded) = args.get(pos + 1) {
            match base64_decode(encoded) {
                Ok(json_str) => {
                    match serde_json::from_str::<crate::config::Settings>(&json_str) {
                        Ok(settings) => {
                            if let Err(e) = settings.save() {
                                eprintln!("Failed to save config: {}", e);
                                std::process::exit(1);
                            }
                            // Success — exit cleanly
                            std::process::exit(0);
                        }
                        Err(e) => {
                            eprintln!("Invalid config JSON: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Invalid base64 config: {}", e);
                    std::process::exit(1);
                }
            }
        }
        eprintln!("Missing config argument after --apply-config");
        std::process::exit(1);
    }

    // Also handle --add-firewall-rules (run from NSIS installer)
    if args.iter().any(|a| a == "--add-firewall-rules") {
        #[cfg(target_os = "windows")]
        add_firewall_rules_impl();
        std::process::exit(0);
    }

    false // Not a CLI invocation — continue with normal Tauri startup
}

// ---------------------------------------------------------------------------
//  Windows implementation — ShellExecuteW "runas"
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn apply_config_elevated_windows(exe: &std::path::Path, encoded: &str) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;

    let verb: Vec<u16> = "runas\0".encode_utf16().collect();
    let exe_w: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let params = format!("--apply-config {}\0", encoded);
    let params_w: Vec<u16> = params.encode_utf16().collect();

    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),   // hwnd
            verb.as_ptr(),          // "runas"
            exe_w.as_ptr(),         // executable
            params_w.as_ptr(),      // arguments
            std::ptr::null(),       // directory
            0,                      // SW_HIDE
        )
    };

    // ShellExecuteW returns > 32 on success
    if result as usize > 32 {
        info!("Elevated config writer launched via UAC");
        // Wait a moment for the elevated process to finish
        std::thread::sleep(std::time::Duration::from_millis(2000));
        Ok(())
    } else {
        warn!("ShellExecuteW failed with code {}", result as usize);
        Err(anyhow::anyhow!(
            "UAC elevation was cancelled or failed (code {})", result as usize
        ))
    }
}

// ---------------------------------------------------------------------------
//  Linux/macOS implementation — pkexec
// ---------------------------------------------------------------------------

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn apply_config_elevated_linux(exe: &std::path::Path, encoded: &str) -> Result<()> {
    use std::process::Command;

    let status = Command::new("pkexec")
        .arg(exe)
        .arg("--apply-config")
        .arg(encoded)
        .status()?;

    if status.success() {
        info!("Elevated config writer completed via pkexec");
        Ok(())
    } else {
        Err(anyhow::anyhow!("pkexec elevation cancelled or failed"))
    }
}

// ---------------------------------------------------------------------------
//  Windows firewall rules
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub fn add_firewall_rules_impl() {
    use std::process::Command;

    let rules = [
        ("Yomie Signal TCP", "21116", "TCP", "in"),
        ("Yomie Signal UDP", "21116", "UDP", "in"),
        ("Yomie Relay",     "21117", "TCP", "in"),
        ("Yomie WS Signal", "21118", "TCP", "in"),
        ("Yomie WS Relay",  "21119", "TCP", "in"),
    ];

    for (name, port, proto, dir) in &rules {
        let _ = Command::new("netsh")
            .args([
                "advfirewall", "firewall", "add", "rule",
                &format!("name={}", name),
                &format!("dir={}", dir),
                "action=allow",
                &format!("protocol={}", proto),
                &format!("localport={}", port),
                "enable=yes",
            ])
            .output();
    }

    // Allow the exe itself for outbound
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new("netsh")
            .args([
                "advfirewall", "firewall", "add", "rule",
                "name=Yomie Client",
                "dir=out",
                "action=allow",
                &format!("program={}", exe.display()),
                "enable=yes",
            ])
            .output();
    }

    info!("Firewall rules added");
}

// ---------------------------------------------------------------------------
//  Base64 helpers (URL-safe, no padding)
// ---------------------------------------------------------------------------

fn base64_encode(input: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(input.as_bytes())
}

fn base64_decode(input: &str) -> Result<String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(input)?;
    Ok(String::from_utf8(bytes)?)
}

/// Request elevation (restart the process with admin/root privileges).
pub fn request_elevation() -> Result<()> {
    if is_elevated() {
        info!("Already running with elevated privileges");
        return Ok(());
    }

    let exe = std::env::current_exe()?;

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;

        let verb: Vec<u16> = "runas\0".encode_utf16().collect();
        let exe_w: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
        let params: Vec<u16> = "--elevated\0".encode_utf16().collect();

        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                exe_w.as_ptr(),
                params.as_ptr(),
                std::ptr::null(),
                5,  // SW_SHOW
            )
        };

        if result as usize > 32 {
            info!("Elevated instance launched — current process will exit");
            std::process::exit(0);
        } else {
            return Err(anyhow::anyhow!("UAC elevation cancelled or failed"));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        warn!("Elevation request — not implemented for this platform");
        Err(anyhow::anyhow!("Privilege elevation not available on this platform"))
    }
}

/// Install as a system service.
pub fn install_service() -> Result<()> {
    warn!("Service installation not yet implemented");
    Err(anyhow::anyhow!(
        "Service installation not yet implemented"
    ))
}

/// Uninstall the system service.
pub fn uninstall_service() -> Result<()> {
    warn!("Service uninstallation not yet implemented");
    Err(anyhow::anyhow!(
        "Service uninstallation not yet implemented"
    ))
}

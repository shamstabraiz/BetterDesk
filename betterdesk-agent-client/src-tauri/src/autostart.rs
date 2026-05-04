//! OS-level autostart management.
//!
//! Wraps `tauri-plugin-autostart` so that our persisted `config.autostart`
//! preference is always mirrored to the operating system:
//! - Linux: `~/.config/autostart/yomie-agent-client.desktop`
//! - Windows: HKCU `Software\Microsoft\Windows\CurrentVersion\Run` entry
//! - macOS: `~/Library/LaunchAgents/com.yomie.agent.plist`
//!
//! The plugin launches the app with the `--autostart` CLI flag so the window
//! stays hidden on boot and only the tray icon is shown.

use log::{info, warn};
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

/// Ensure the OS autostart registration matches `enabled`.
///
/// Safe to call multiple times — both `enable()` and `disable()` are idempotent
/// in the plugin. Logs a warning on failure but never panics; persisted config
/// remains authoritative even if the OS hook fails (e.g. read-only profile).
pub fn sync_os_autostart(app: &AppHandle, enabled: bool) {
    let manager = app.autolaunch();

    match manager.is_enabled() {
        Ok(current) if current == enabled => {
            info!(
                "[autostart] Already {} — no change",
                if enabled { "enabled" } else { "disabled" }
            );
            return;
        }
        Ok(_) => {}
        Err(e) => {
            warn!("[autostart] Failed to query state: {}", e);
        }
    }

    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    match result {
        Ok(()) => info!(
            "[autostart] OS registration {}",
            if enabled { "enabled" } else { "disabled" }
        ),
        Err(e) => warn!(
            "[autostart] Failed to {}: {}",
            if enabled { "enable" } else { "disable" },
            e
        ),
    }
}

use anyhow::Result;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persistent agent configuration stored as JSON on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    // ── Server connection ───────────────────────────────────────────────────

    /// Server address entered by the user.
    ///
    /// Normalized registrations persist a full origin such as
    /// "https://betterdesk.example.com:5443" so follow-up API/CDAP calls can
    /// preserve the correct transport scheme.
    pub server_address: String,

    /// API key used to authenticate the sidecar Go agent with the CDAP gateway.
    /// Obtained from the BetterDesk admin panel → API Keys.
    #[serde(default)]
    pub api_key: String,

    /// CDAP WebSocket port (default 21122).
    #[serde(default = "default_cdap_port")]
    pub cdap_port: u16,

    // ── Device identity ─────────────────────────────────────────────────────

    /// Unique device identifier assigned during registration.
    pub device_id: String,

    /// Device display name (defaults to hostname).
    pub device_name: String,

    /// Optional server-issued device token for CDAP authentication.
    /// Empty for the current enrollment flow unless the server explicitly
    /// returns such a token.
    pub auth_token: String,

    /// Whether the device has completed registration.
    pub registered: bool,

    // ── Capability gates ────────────────────────────────────────────────────
    // These control what the operator can do remotely. The user can change
    // them from the Settings panel. They are forwarded to the Go sidecar via
    // the go-agent-config.json file.

    /// Allow operators to view and control the screen (remote desktop).
    /// Maps to `screenshot` in Go agent config (current JPEG mode).
    #[serde(default = "default_true")]
    pub allow_screen_capture: bool,

    /// Require explicit user consent dialog before a remote session starts.
    #[serde(default = "default_true")]
    pub require_consent: bool,

    /// Allow operators to open a terminal on this device.
    #[serde(default = "default_true")]
    pub allow_terminal: bool,

    /// Allow operators to browse and transfer files.
    #[serde(default = "default_true")]
    pub allow_file_browser: bool,

    /// Allow clipboard sync between operator and this device.
    #[serde(default = "default_true")]
    pub allow_clipboard: bool,

    // ── Sidecar auto-start ──────────────────────────────────────────────────

    /// Start the Go sidecar agent automatically when Tauri app starts.
    /// Disable only for debugging — without the sidecar the device is invisible
    /// to operators (no CDAP connection, no screen sharing, no terminal).
    #[serde(default = "default_true")]
    pub auto_start_sidecar: bool,

    // ── General preferences ─────────────────────────────────────────────────

    /// Start Tauri app on system boot.
    pub autostart: bool,

    /// Minimize to system tray on startup (recommended — agent runs in background).
    pub start_minimized: bool,

    /// UI language code ("en" or "pl").
    pub language: String,
}

fn default_cdap_port() -> u16 { 21122 }
fn default_true() -> bool { true }

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            server_address: String::new(),
            api_key: String::new(),
            cdap_port: 21122,
            device_id: String::new(),
            device_name: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            auth_token: String::new(),
            registered: false,
            allow_screen_capture: true,
            require_consent: true,
            allow_terminal: true,
            allow_file_browser: true,
            allow_clipboard: true,
            auto_start_sidecar: true,
            autostart: true,
            start_minimized: true,
            language: "en".to_string(),
        }
    }
}

impl AgentConfig {
    /// Configuration file path.
    fn config_path() -> PathBuf {
        let dir = directories::ProjectDirs::from("com", "betterdesk", "agent")
            .map(|d| d.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        dir.join("agent-config.json")
    }

    /// Load config from disk. Returns default if file doesn't exist.
    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            info!("No config file at {:?} — using defaults", path);
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        let config: Self = serde_json::from_str(&content)?;
        Ok(config)
    }

    /// Repair stale configs produced by the legacy fake-registration flow.
    ///
    /// Older clients marked the device as registered after a heartbeat ACK and
    /// minted a local placeholder token (`BD-TOKEN-*`). That state is invalid:
    /// no peer exists on the server and the sidecar cannot authenticate.
    pub fn repair_legacy_registration_state(&mut self) -> Result<bool> {
        if self.registered && self.api_key.is_empty() && self.auth_token.starts_with("BD-TOKEN-") {
            warn!(
                "Detected legacy placeholder registration token for {:?}; resetting local enrollment state",
                self.device_id
            );

            let old_device_id = self.device_id.clone();
            self.registered = false;
            self.device_id.clear();
            self.auth_token.clear();

            if !old_device_id.is_empty() {
                Self::clear_token_secure(&old_device_id);
            }

            self.save()?;
            return Ok(true);
        }

        Ok(false)
    }

    /// Persist config to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        info!("Config saved to {:?}", path);
        Ok(())
    }

    /// Whether this device has completed registration with a server.
    pub fn is_registered(&self) -> bool {
        self.registered && !self.device_id.is_empty() && !self.server_address.is_empty()
    }

    /// Build a `SidecarConfig` from this config (needed by `SidecarManager::start`).
    pub fn to_sidecar_config(&self) -> crate::sidecar::SidecarConfig {
        let data_dir = directories::ProjectDirs::from("com", "betterdesk", "agent")
            .map(|d| d.data_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        crate::sidecar::SidecarConfig {
            server_address: self.server_address.clone(),
            device_id: self.device_id.clone(),
            device_name: self.device_name.clone(),
            api_key: self.api_key.clone(),
            auth_token: self.auth_token.clone(),
            allow_terminal: self.allow_terminal,
            allow_file_browser: self.allow_file_browser,
            allow_clipboard: self.allow_clipboard,
            allow_screen_capture: self.allow_screen_capture,
            data_dir,
            cdap_port: self.cdap_port,
        }
    }

    /// Store credentials securely via OS keyring.
    pub fn store_token_secure(&self) -> Result<()> {
        if self.auth_token.is_empty() {
            return Ok(());
        }
        let entry = keyring::Entry::new("betterdesk-agent", &self.device_id)?;
        entry.set_password(&self.auth_token)?;
        info!("Auth token stored in OS keyring");
        Ok(())
    }

    /// Retrieve token from OS keyring.
    pub fn load_token_secure(device_id: &str) -> Option<String> {
        keyring::Entry::new("betterdesk-agent", device_id)
            .ok()
            .and_then(|e| e.get_password().ok())
    }

    /// Delete token from OS keyring.
    pub fn clear_token_secure(device_id: &str) {
        if let Ok(entry) = keyring::Entry::new("betterdesk-agent", device_id) {
            let _ = entry.delete_credential();
        }
    }
}

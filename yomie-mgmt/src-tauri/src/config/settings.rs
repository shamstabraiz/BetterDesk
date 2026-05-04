//! Persistent application settings.
//!
//! Stored as JSON in the platform-specific config directory:
//!   - Windows: %APPDATA%/Yomie/config.json
//!   - Linux:   ~/.config/yomie/config.json
//!   - macOS:   ~/Library/Application Support/Yomie/config.json

use anyhow::{Context, Result};
use log::warn;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use super::secure_store;

/// Generate a random 8-character alphanumeric device password.
fn generate_device_password() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";
    let mut rng = rand::thread_rng();
    (0..8).map(|_| CHARSET[rng.gen_range(0..CHARSET.len())] as char).collect()
}

/// Application settings persisted to disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Server address (host:port for hbbs)
    pub server_address: String,
    /// Relay server address (host:port for hbbr), empty = same as server
    pub relay_address: String,
    /// Server public key (base64-encoded Ed25519)
    pub server_key: String,
    /// API port for Yomie HTTP API
    pub api_port: u16,
    /// Whether to use RustDesk-compatible protocol
    pub rustdesk_compat: bool,
    /// Whether to use Yomie native protocol (enhanced security)
    pub native_protocol: bool,
    /// Whether to force relay (no direct P2P)
    pub force_relay: bool,
    /// Preferred video codec
    pub preferred_codec: String,
    /// Maximum FPS
    pub max_fps: u32,
    /// Image quality (0=auto, 1=low, 2=balanced, 3=best)
    pub image_quality: u32,
    /// Disable audio
    pub disable_audio: bool,
    /// Language code (en, pl, etc.)
    pub language: String,
    /// Theme (dark, light, system)
    pub theme: String,
    /// Whether to start minimized
    pub start_minimized: bool,
    /// Whether to run as system service (for unattended access)
    pub run_as_service: bool,
    /// Custom device password (for incoming connections)
    pub device_password: String,
    /// Trusted server certificates (PEM, for certificate pinning)
    pub pinned_certificates: Vec<String>,
    /// Yomie console URL (e.g. http://192.168.0.110:5000)
    pub console_url: String,
    /// Access token for Yomie API authentication
    pub access_token: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server_address: String::new(),
            relay_address: String::new(),
            server_key: String::new(),
            api_port: 21114,
            rustdesk_compat: true,
            native_protocol: true,
            force_relay: false,
            preferred_codec: "auto".into(),
            max_fps: 30,
            image_quality: 0,
            disable_audio: false,
            language: "en".into(),
            theme: "dark".into(),
            start_minimized: false,
            run_as_service: false,
            device_password: String::new(),
            pinned_certificates: Vec::new(),
            console_url: String::new(),
            access_token: None,
        }
    }
}

impl Settings {
    /// Get the config file path.
    pub fn config_path() -> Result<PathBuf> {
        let dirs = directories::ProjectDirs::from("com", "Yomie", "Yomie")
            .context("Cannot determine config directory")?;
        let config_dir = dirs.config_dir();
        fs::create_dir_all(config_dir)
            .with_context(|| format!("Cannot create config dir: {}", config_dir.display()))?;
        Ok(config_dir.join("config.json"))
    }

    /// Load settings from disk (or return defaults if file doesn't exist).
    ///
    /// If `device_password` is empty, a random 8-character alphanumeric
    /// password is generated and persisted. This ensures incoming remote
    /// connections can authenticate via the RustDesk relay protocol.
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        let mut settings = if !path.exists() {
            Self::default()
        } else {
            let data = fs::read_to_string(&path)
                .with_context(|| format!("Cannot read config: {}", path.display()))?;
            serde_json::from_str(&data)
                .with_context(|| format!("Cannot parse config: {}", path.display()))?
        };

        let mut migrated = false;

        match secure_store::load_access_token() {
            Ok(Some(token)) => settings.access_token = Some(token),
            Ok(None) => {
                if let Some(token) = settings.access_token.clone().filter(|v| !v.is_empty()) {
                    if let Err(err) = secure_store::store_access_token(Some(&token)) {
                        warn!("Failed to migrate access token to secure storage: {}", err);
                    } else {
                        migrated = true;
                    }
                }
            }
            Err(err) => warn!("Failed to load access token from secure storage: {}", err),
        }

        match secure_store::load_device_password() {
            Ok(Some(password)) => settings.device_password = password,
            Ok(None) => {
                if settings.device_password.is_empty() {
                    settings.device_password = generate_device_password();
                }
                if let Err(err) = secure_store::store_device_password(Some(&settings.device_password)) {
                    warn!("Failed to persist device password to secure storage: {}", err);
                } else {
                    migrated = true;
                }
            }
            Err(err) => {
                warn!("Failed to load device password from secure storage: {}", err);
                if settings.device_password.is_empty() {
                    settings.device_password = generate_device_password();
                }
            }
        }

        // Auto-generate device password on first run
        if settings.device_password.is_empty() {
            settings.device_password = generate_device_password();
            log::info!("Generated device password (shown in Settings)");
            // Persist immediately so the password survives restarts
            let _ = settings.save();
        } else if migrated {
            let _ = settings.save();
        }

        Ok(settings)
    }

    /// Save settings to disk with restrictive permissions.
    pub fn save(&self) -> Result<()> {
        secure_store::store_access_token(self.access_token.as_deref())?;
        secure_store::store_device_password(Some(&self.device_password))?;

        let path = Self::config_path()?;
        let mut persisted = self.clone();
        persisted.access_token = None;
        persisted.device_password.clear();
        let data = serde_json::to_string_pretty(&persisted)?;
        fs::write(&path, &data)
            .with_context(|| format!("Cannot write config: {}", path.display()))?;

        // Restrict file permissions to owner-only (Unix)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600);
            let _ = fs::set_permissions(&path, perms);
        }

        Ok(())
    }

    /// Get the effective relay address (falls back to server address if empty).
    pub fn effective_relay_address(&self) -> String {
        if self.relay_address.is_empty() {
            // Replace hbbs port (21116) with hbbr port (21117) in server address
            if let Some(colon) = self.server_address.rfind(':') {
                let host = &self.server_address[..colon];
                return format!("{}:21117", host);
            }
            format!("{}:21117", self.server_address)
        } else {
            self.relay_address.clone()
        }
    }

    /// Get hbbs host:port for TCP signaling.
    pub fn hbbs_address(&self) -> String {
        if self.server_address.contains(':') {
            self.server_address.clone()
        } else {
            format!("{}:21116", self.server_address)
        }
    }

    /// Get the Yomie console API base URL (for native protocol).
    ///
    /// Uses `console_url` if set, otherwise derives from `server_address`.
    pub fn bd_api_url(&self) -> String {
        if !self.console_url.is_empty() {
            let url = self.console_url.trim_end_matches('/');
            // Ensure scheme is present — reqwest requires full URLs
            if url.starts_with("http://") || url.starts_with("https://") {
                return url.to_string();
            }
            return format!("http://{}", url);
        }
        // Derive from server_address — assume port 5000
        let host = if let Some(colon) = self.server_address.rfind(':') {
            &self.server_address[..colon]
        } else if self.server_address.is_empty() {
            "localhost"
        } else {
            &self.server_address
        };
        format!("http://{}:5000", host)
    }

    /// Get the Go server HTTP API base URL (port 21114).
    ///
    /// The Go server manages the peers table and runs on signal_port - 2.
    /// RustDesk clients send heartbeat/sysinfo to this port.
    pub fn go_api_url(&self) -> String {
        let host = self.server_host();
        format!("http://{}:21114", host)
    }

    /// Extract just the host portion from `server_address` (strips port).
    fn server_host(&self) -> &str {
        if let Some(colon) = self.server_address.rfind(':') {
            &self.server_address[..colon]
        } else if self.server_address.is_empty() {
            "localhost"
        } else {
            &self.server_address
        }
    }

    /// Get `host:port` for WebSocket connections to the Yomie console.
    ///
    /// Used to build `ws://{server_ws_host()}/ws/chat/...` and similar URLs.
    pub fn server_ws_host(&self) -> String {
        if !self.console_url.is_empty() {
            // Strip http(s):// scheme and trailing slash, keep host:port
            let stripped = self.console_url
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/');
            return stripped.to_string();
        }
        let host = if let Some(colon) = self.server_address.rfind(':') {
            &self.server_address[..colon]
        } else if self.server_address.is_empty() {
            "localhost"
        } else {
            &self.server_address
        };
        format!("{}:5000", host)
    }

    /// Get the WebSocket scheme (`ws` or `wss`) based on the console URL.
    pub fn server_ws_scheme(&self) -> &str {
        if self.console_url.starts_with("https://") {
            "wss"
        } else {
            "ws"
        }
    }
}

/// Yomie Agent — USB Device Monitor
///
/// Enumerates connected USB devices and detects insertion/removal events.
/// Reports device info (VID, PID, serial, class) to the server for
/// policy evaluation.

use std::collections::HashMap;

/// Represents a detected USB device.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsbDevice {
    /// Vendor ID (hex)
    pub vid: String,
    /// Product ID (hex)
    pub pid: String,
    /// Device serial number (if available)
    pub serial: Option<String>,
    /// Human-readable description / product name
    pub description: String,
    /// Device class (e.g. "mass_storage", "hid", "printer", "audio")
    pub device_class: String,
    /// Drive letter or mount point (for mass storage only)
    pub mount_point: Option<String>,
    /// Whether the device is currently connected
    pub connected: bool,
    /// Timestamp of first detection (epoch millis)
    pub first_seen: u64,
}

/// Event emitted when a USB device is inserted or removed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsbEvent {
    pub event_type: UsbEventType,
    pub device: UsbDevice,
    pub timestamp: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum UsbEventType {
    Connected,
    Disconnected,
}

/// Monitors USB devices by polling the OS device list periodically.
pub struct UsbMonitor {
    /// Previously known devices indexed by a composite key (vid:pid:serial)
    known_devices: HashMap<String, UsbDevice>,
    /// Polling interval in seconds
    poll_interval_secs: u64,
}

impl UsbMonitor {
    pub fn new(poll_interval_secs: u64) -> Self {
        Self {
            known_devices: HashMap::new(),
            poll_interval_secs: poll_interval_secs.max(5),
        }
    }

    /// Return the configured poll interval.
    pub fn interval_secs(&self) -> u64 {
        self.poll_interval_secs
    }

    /// Poll current USB devices and return a list of change events.
    pub fn poll(&mut self) -> Vec<UsbEvent> {
        let current = Self::enumerate_devices();
        let mut events = Vec::new();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Detect newly connected devices
        for (key, dev) in &current {
            if !self.known_devices.contains_key(key) {
                events.push(UsbEvent {
                    event_type: UsbEventType::Connected,
                    device: dev.clone(),
                    timestamp: now,
                });
            }
        }

        // Detect disconnected devices
        for (key, dev) in &self.known_devices {
            if !current.contains_key(key) {
                let mut d = dev.clone();
                d.connected = false;
                events.push(UsbEvent {
                    event_type: UsbEventType::Disconnected,
                    device: d,
                    timestamp: now,
                });
            }
        }

        self.known_devices = current;
        events
    }

    /// Get all currently known devices.
    pub fn current_devices(&self) -> Vec<&UsbDevice> {
        self.known_devices.values().collect()
    }

    // -----------------------------------------------------------------------
    //  Platform-specific enumeration
    // -----------------------------------------------------------------------

    #[cfg(target_os = "windows")]
    fn enumerate_devices() -> HashMap<String, UsbDevice> {
        // On Windows, use WMI via PowerShell to list USB devices.
        // A full implementation would use SetupAPI / WMI COM bindings.
        // Here we use a lightweight approach via command output parsing.
        let mut devices = HashMap::new();

        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile", "-Command",
                r#"Get-CimInstance Win32_USBControllerDevice |
                   ForEach-Object { [wmi]($_.Dependent) } |
                   Select-Object DeviceID, Name, Status |
                   ConvertTo-Json -Compress"#,
            ])
            .output();

        if let Ok(out) = output {
            if let Ok(text) = String::from_utf8(out.stdout) {
                if let Ok(items) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                    for item in items {
                        let device_id = item["DeviceID"].as_str().unwrap_or_default();
                        let name = item["Name"].as_str().unwrap_or("Unknown").to_string();
                        let (vid, pid, serial) = parse_device_id(device_id);
                        let key = format!("{}:{}:{}", vid, pid, serial.as_deref().unwrap_or(""));
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;

                        devices.insert(key, UsbDevice {
                            vid,
                            pid,
                            serial,
                            description: name,
                            device_class: classify_device(device_id),
                            mount_point: None,
                            connected: true,
                            first_seen: now_ms,
                        });
                    }
                }
                // WMI may return a single object instead of array
                else if let Ok(item) = serde_json::from_str::<serde_json::Value>(&text) {
                    if item.is_object() {
                        let device_id = item["DeviceID"].as_str().unwrap_or_default();
                        let name = item["Name"].as_str().unwrap_or("Unknown").to_string();
                        let (vid, pid, serial) = parse_device_id(device_id);
                        let key = format!("{}:{}:{}", vid, pid, serial.as_deref().unwrap_or(""));
                        let now_ms = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_millis() as u64;
                        devices.insert(key, UsbDevice {
                            vid,
                            pid,
                            serial,
                            description: name,
                            device_class: classify_device(device_id),
                            mount_point: None,
                            connected: true,
                            first_seen: now_ms,
                        });
                    }
                }
            }
        }

        devices
    }

    #[cfg(not(target_os = "windows"))]
    fn enumerate_devices() -> HashMap<String, UsbDevice> {
        // On Linux, read /sys/bus/usb/devices/
        let mut devices = HashMap::new();
        let usb_path = std::path::Path::new("/sys/bus/usb/devices");
        if !usb_path.exists() {
            return devices;
        }

        if let Ok(entries) = std::fs::read_dir(usb_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                let vid = read_sysfs_file(&path.join("idVendor"));
                let pid = read_sysfs_file(&path.join("idProduct"));
                if vid.is_empty() || pid.is_empty() {
                    continue;
                }
                let serial = {
                    let s = read_sysfs_file(&path.join("serial"));
                    if s.is_empty() { None } else { Some(s) }
                };
                let product = read_sysfs_file(&path.join("product"));
                let dev_class = read_sysfs_file(&path.join("bDeviceClass"));

                let key = format!("{}:{}:{}", vid, pid, serial.as_deref().unwrap_or(""));
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                devices.insert(key, UsbDevice {
                    vid,
                    pid,
                    serial,
                    description: if product.is_empty() { "USB Device".into() } else { product },
                    device_class: classify_class_code(&dev_class),
                    mount_point: None,
                    connected: true,
                    first_seen: now_ms,
                });
            }
        }

        devices
    }
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

/// Parse a Windows device ID like `USB\VID_1234&PID_5678\SERIAL` into (vid, pid, serial).
fn parse_device_id(device_id: &str) -> (String, String, Option<String>) {
    let upper = device_id.to_uppercase();
    let vid = upper
        .find("VID_")
        .map(|i| &upper[i + 4..])
        .and_then(|s| s.split(&['&', '\\'][..]).next())
        .unwrap_or("")
        .to_string();
    let pid = upper
        .find("PID_")
        .map(|i| &upper[i + 4..])
        .and_then(|s| s.split(&['&', '\\'][..]).next())
        .unwrap_or("")
        .to_string();

    // Serial is typically the last segment after the second backslash
    let parts: Vec<&str> = device_id.split('\\').collect();
    let serial = if parts.len() >= 3 && !parts[2].is_empty() {
        Some(parts[2].to_string())
    } else {
        None
    };

    (vid, pid, serial)
}

/// Classify a device based on its device ID string (Windows).
fn classify_device(device_id: &str) -> String {
    let lower = device_id.to_lowercase();
    if lower.contains("disk") || lower.contains("storage") || lower.contains("usbstor") {
        "mass_storage".into()
    } else if lower.contains("hid") || lower.contains("keyboard") || lower.contains("mouse") {
        "hid".into()
    } else if lower.contains("print") {
        "printer".into()
    } else if lower.contains("audio") || lower.contains("sound") {
        "audio".into()
    } else if lower.contains("video") || lower.contains("camera") {
        "video".into()
    } else if lower.contains("net") || lower.contains("bluetooth") {
        "network".into()
    } else {
        "other".into()
    }
}

/// Classify based on USB class code (Linux sysfs bDeviceClass).
#[allow(dead_code)]
fn classify_class_code(code: &str) -> String {
    match code.trim() {
        "08" => "mass_storage".into(),
        "03" => "hid".into(),
        "07" => "printer".into(),
        "01" => "audio".into(),
        "0e" | "0E" => "video".into(),
        "e0" | "E0" => "wireless".into(),
        "02" => "comm".into(),
        _ => "other".into(),
    }
}

/// Read a single-line sysfs attribute file.
#[allow(dead_code)]
fn read_sysfs_file(path: &std::path::Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .trim()
        .to_string()
}

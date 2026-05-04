//! Yomie Activity Tracker — foreground app & idle detection
//!
//! Polls the foreground window title and application name every N seconds.
//! Detects user idle time via system APIs (GetLastInputInfo on Windows,
//! X11/XScreenSaver on Linux).

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Snapshot of the current foreground application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForegroundSnapshot {
    pub app_name: String,
    pub window_title: String,
    pub timestamp: String,
}

/// How long the user has been idle (no keyboard/mouse input).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleInfo {
    pub idle_seconds: u64,
    pub is_idle: bool,
}

/// Default idle threshold in seconds (5 minutes).
const IDLE_THRESHOLD_SECS: u64 = 300;

/// Default polling interval in seconds.
const POLL_INTERVAL_SECS: u64 = 5;

/// Activity tracker that polls the active window and idle state.
pub struct ActivityTracker {
    poll_interval: Duration,
    idle_threshold: Duration,
    last_poll: Option<Instant>,
}

impl ActivityTracker {
    pub fn new() -> Self {
        Self {
            poll_interval: Duration::from_secs(POLL_INTERVAL_SECS),
            idle_threshold: Duration::from_secs(IDLE_THRESHOLD_SECS),
            last_poll: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_intervals(poll_secs: u64, idle_threshold_secs: u64) -> Self {
        Self {
            poll_interval: Duration::from_secs(poll_secs),
            idle_threshold: Duration::from_secs(idle_threshold_secs),
            last_poll: None,
        }
    }

    pub fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    /// Check if enough time has passed since the last poll.
    pub fn should_poll(&mut self) -> bool {
        match self.last_poll {
            None => {
                self.last_poll = Some(Instant::now());
                true
            }
            Some(last) => {
                if last.elapsed() >= self.poll_interval {
                    self.last_poll = Some(Instant::now());
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Get current foreground window info (platform-specific).
    pub fn get_foreground(&self) -> ForegroundSnapshot {
        let (app, title) = get_foreground_window();
        ForegroundSnapshot {
            app_name: app,
            window_title: title,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Get current idle time.
    pub fn get_idle(&self) -> IdleInfo {
        let idle_secs = get_idle_seconds();
        IdleInfo {
            idle_seconds: idle_secs,
            is_idle: idle_secs >= self.idle_threshold.as_secs(),
        }
    }
}

// ---------------------------------------------------------------------------
//  Platform-specific foreground window detection
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn get_foreground_window() -> (String, String) {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    unsafe {
        let hwnd = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        if hwnd.is_null() {
            return ("Unknown".into(), "".into());
        }

        // Window title
        let mut title_buf = [0u16; 512];
        let title_len = windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW(
            hwnd,
            title_buf.as_mut_ptr(),
            title_buf.len() as i32,
        );
        let title = if title_len > 0 {
            OsString::from_wide(&title_buf[..title_len as usize])
                .to_string_lossy()
                .into_owned()
        } else {
            String::new()
        };

        // Process name
        let mut pid: u32 = 0;
        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, &mut pid);

        let process_name = if pid > 0 {
            get_process_name_by_pid(pid)
        } else {
            "Unknown".to_string()
        };

        (process_name, title)
    }
}

#[cfg(target_os = "windows")]
unsafe fn get_process_name_by_pid(pid: u32) -> String {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;

    let handle = windows_sys::Win32::System::Threading::OpenProcess(
        windows_sys::Win32::System::Threading::PROCESS_QUERY_LIMITED_INFORMATION,
        0,
        pid,
    );
    if handle.is_null() {
        return "Unknown".to_string();
    }

    let mut buf = [0u16; 260];
    let mut size = buf.len() as u32;
    let ok = windows_sys::Win32::System::Threading::QueryFullProcessImageNameW(
        handle,
        0,
        buf.as_mut_ptr(),
        &mut size,
    );
    windows_sys::Win32::Foundation::CloseHandle(handle);

    if ok != 0 && size > 0 {
        let path = OsString::from_wide(&buf[..size as usize])
            .to_string_lossy()
            .into_owned();
        // Extract just the filename
        path.rsplit('\\')
            .next()
            .unwrap_or(&path)
            .to_string()
    } else {
        "Unknown".to_string()
    }
}

#[cfg(target_os = "linux")]
fn get_foreground_window() -> (String, String) {
    // Use xdotool to get the active window
    let output = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowname"])
        .output();

    let title = match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        }
        _ => String::new(),
    };

    let output2 = std::process::Command::new("xdotool")
        .args(["getactivewindow", "getwindowpid"])
        .output();

    let app_name = match output2 {
        Ok(out) if out.status.success() => {
            let pid_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if let Ok(pid) = pid_str.parse::<u32>() {
                let comm_path = format!("/proc/{}/comm", pid);
                std::fs::read_to_string(&comm_path)
                    .unwrap_or_else(|_| "Unknown".into())
                    .trim()
                    .to_string()
            } else {
                "Unknown".into()
            }
        }
        _ => "Unknown".into(),
    };

    (app_name, title)
}

#[cfg(target_os = "macos")]
fn get_foreground_window() -> (String, String) {
    let script = r#"
    tell application "System Events"
        set frontApp to name of first application process whose frontmost is true
        set frontTitle to ""
        try
            tell process frontApp
                set frontTitle to name of front window
            end tell
        end try
        return frontApp & "|" & frontTitle
    end tell
    "#;

    let output = std::process::Command::new("osascript")
        .args(["-e", script])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let parts: Vec<&str> = result.splitn(2, '|').collect();
            let app = parts.first().unwrap_or(&"Unknown").to_string();
            let title = parts.get(1).unwrap_or(&"").to_string();
            (app, title)
        }
        _ => ("Unknown".into(), String::new()),
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn get_foreground_window() -> (String, String) {
    ("Unknown".into(), String::new())
}

// ---------------------------------------------------------------------------
//  Platform-specific idle detection
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
fn get_idle_seconds() -> u64 {
    unsafe {
        let mut last_input = windows_sys::Win32::UI::Input::KeyboardAndMouse::LASTINPUTINFO {
            cbSize: std::mem::size_of::<windows_sys::Win32::UI::Input::KeyboardAndMouse::LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if windows_sys::Win32::UI::Input::KeyboardAndMouse::GetLastInputInfo(&mut last_input) != 0 {
            let tick = windows_sys::Win32::System::SystemInformation::GetTickCount();
            let idle_ms = tick.wrapping_sub(last_input.dwTime);
            (idle_ms / 1000) as u64
        } else {
            0
        }
    }
}

#[cfg(target_os = "linux")]
fn get_idle_seconds() -> u64 {
    // Try xprintidle (common utility)
    let output = std::process::Command::new("xprintidle").output();
    match output {
        Ok(out) if out.status.success() => {
            let ms_str = String::from_utf8_lossy(&out.stdout).trim().to_string();
            ms_str.parse::<u64>().unwrap_or(0) / 1000
        }
        _ => 0,
    }
}

#[cfg(target_os = "macos")]
fn get_idle_seconds() -> u64 {
    let output = std::process::Command::new("ioreg")
        .args(["-c", "IOHIDSystem", "-d", "4"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            // Parse HIDIdleTime from ioreg output (nanoseconds)
            for line in text.lines() {
                if line.contains("HIDIdleTime") {
                    if let Some(val) = line.split('=').last() {
                        let val = val.trim().trim_matches('"');
                        if let Ok(ns) = val.parse::<u64>() {
                            return ns / 1_000_000_000;
                        }
                    }
                }
            }
            0
        }
        _ => 0,
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn get_idle_seconds() -> u64 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_should_poll() {
        let mut tracker = ActivityTracker::with_intervals(0, 300);
        assert!(tracker.should_poll());
        assert!(tracker.should_poll()); // 0-second interval → always true
    }

    #[test]
    fn test_foreground_snapshot() {
        let tracker = ActivityTracker::new();
        let snap = tracker.get_foreground();
        // Just verify it returns something, platform-specific
        assert!(!snap.timestamp.is_empty());
    }
}

//! Dual logging system — writes to both stderr (console) and a rotating log file.
//!
//! Log file location: `<config_dir>/logs/yomie-mgmt.log`
//! - Windows: `%APPDATA%\Yomie\Yomie\logs\yomie-mgmt.log`
//! - Linux:   `~/.config/Yomie/logs/yomie-mgmt.log`
//!
//! Features:
//! - Dual target (stderr + file) for both --console and headless operation
//! - Auto-rotation: renames to `.log.1` when file exceeds 5 MB
//! - Millisecond timestamps, module path, log level
//! - Thread-safe via `std::sync::Mutex`
//! - IPC bridge: frontend can write to the same log via `write_log` command

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use chrono::Local;
use log::{Level, LevelFilter, Log, Metadata, Record};

/// Maximum log file size before rotation (5 MB).
const MAX_LOG_SIZE: u64 = 5 * 1024 * 1024;

/// Shared log file handle — initialized once at startup.
static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

/// Cached log file path for open/rotate operations.
static LOG_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Our custom logger that writes to stderr + file simultaneously.
struct DualLogger {
    level: LevelFilter,
}

impl Log for DualLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let now = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let level = record.level();
        let target = record.target();
        // Shorten module paths: betterdesk_mgmt_lib::commands → commands
        let short_target = target
            .strip_prefix("betterdesk_mgmt_lib::")
            .unwrap_or(target);

        let line = format!("{} [{}] {} — {}\n", now, level, short_target, record.args());

        // Write to stderr (visible in --console mode and IDE terminals)
        let _ = std::io::stderr().write_all(line.as_bytes());

        // Write to file
        if let Ok(mut guard) = LOG_FILE.lock() {
            if let Some(ref mut file) = *guard {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            }
        }
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
        if let Ok(mut guard) = LOG_FILE.lock() {
            if let Some(ref mut file) = *guard {
                let _ = file.flush();
            }
        }
    }
}

/// Initialize the dual logging system.
///
/// - `is_console`: if true, default level is DEBUG; otherwise INFO.
///
/// Call once at application startup (before any `log::info!()` etc.).
pub fn init(is_console: bool) {
    let level = if is_console {
        LevelFilter::Debug
    } else {
        // Check RUST_LOG env for custom level
        std::env::var("RUST_LOG")
            .ok()
            .and_then(|v| v.parse::<LevelFilter>().ok())
            .unwrap_or(LevelFilter::Info)
    };

    // Create log directory and file
    let log_path = get_log_dir().join("yomie-mgmt.log");
    if let Some(parent) = log_path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    // Rotate if existing file is too large
    rotate_if_needed(&log_path);

    // Open log file in append mode
    match OpenOptions::new().create(true).append(true).open(&log_path) {
        Ok(file) => {
            if let Ok(mut guard) = LOG_FILE.lock() {
                *guard = Some(file);
            }
            if let Ok(mut guard) = LOG_PATH.lock() {
                *guard = Some(log_path.clone());
            }
        }
        Err(e) => {
            eprintln!("[logging] Failed to open log file {}: {}", log_path.display(), e);
        }
    }

    // Register our logger
    let logger = DualLogger { level };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
    }
}

/// Get the log directory path.
fn get_log_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "Yomie", "Yomie")
        .map(|d| d.config_dir().join("logs"))
        .unwrap_or_else(|| PathBuf::from("logs"))
}

/// Rotate log file if it exceeds MAX_LOG_SIZE.
fn rotate_if_needed(path: &PathBuf) {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > MAX_LOG_SIZE {
            let backup = path.with_extension("log.1");
            let _ = fs::remove_file(&backup); // remove old backup
            let _ = fs::rename(path, &backup);
        }
    }
}

/// Get the current log file path (for `open_log_file` IPC).
pub fn log_file_path() -> Option<PathBuf> {
    LOG_PATH.lock().ok().and_then(|g| g.clone())
}

/// Write a line from the frontend (JS) into the shared log file.
/// Called via the `write_log` IPC command.
pub fn write_frontend_log(level: Level, module: &str, message: &str) {
    let now = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("{} [{}] [JS] {} — {}\n", now, level, module, message);

    // stderr
    let _ = std::io::stderr().write_all(line.as_bytes());

    // file
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut file) = *guard {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }
}

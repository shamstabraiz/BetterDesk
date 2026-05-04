/// Yomie Agent — File Watcher (DLP)
///
/// Watches file operations on removable drives and monitored directories.
/// Logs copy, move, delete, and rename events for audit trail.
/// Uses polling-based approach for cross-platform compatibility.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A logged file operation event.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileEvent {
    pub event_type: FileEventType,
    pub path: String,
    pub filename: String,
    pub size_bytes: u64,
    pub drive_label: String,
    pub drive_type: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum FileEventType {
    Created,
    Modified,
    Deleted,
    Renamed,
}

impl std::fmt::Display for FileEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Created => write!(f, "created"),
            Self::Modified => write!(f, "modified"),
            Self::Deleted => write!(f, "deleted"),
            Self::Renamed => write!(f, "renamed"),
        }
    }
}

/// Snapshot of a single file for change detection.
#[derive(Debug, Clone)]
struct FileSnapshot {
    size: u64,
    modified: SystemTime,
}

/// Watches a set of directories for file changes via polling.
pub struct FileWatcher {
    /// Directories to watch
    watch_dirs: Vec<PathBuf>,
    /// Previous snapshot of files: path → snapshot
    snapshots: HashMap<PathBuf, FileSnapshot>,
    /// Accumulated events since last drain
    events: Vec<FileEvent>,
    /// Max file events to keep in buffer
    max_buffer: usize,
}

impl FileWatcher {
    /// Create a new watcher for the given directories.
    pub fn new(dirs: Vec<PathBuf>, max_buffer: usize) -> Self {
        Self {
            watch_dirs: dirs,
            snapshots: HashMap::new(),
            events: Vec::new(),
            max_buffer: max_buffer.max(100),
        }
    }

    /// Add a directory to the watch list.
    pub fn add_dir(&mut self, dir: PathBuf) {
        if !self.watch_dirs.contains(&dir) {
            self.watch_dirs.push(dir);
        }
    }

    /// Remove a directory from the watch list.
    pub fn remove_dir(&mut self, dir: &Path) {
        self.watch_dirs.retain(|d| d != dir);
        // Remove snapshots for files under this dir
        self.snapshots.retain(|p, _| !p.starts_with(dir));
    }

    /// Poll all watched directories and detect changes.
    /// Returns the number of new events detected.
    pub fn poll(&mut self) -> usize {
        let now = now_millis();

        let mut current_files: HashMap<PathBuf, FileSnapshot> = HashMap::new();

        let dirs: Vec<PathBuf> = self.watch_dirs.clone();
        for dir in &dirs {
            if !dir.exists() || !dir.is_dir() {
                continue;
            }
            Self::scan_dir(dir, &mut current_files);
        }

        // Detect created or modified files
        let mut new_event_list: Vec<FileEvent> = Vec::new();
        for (path, snap) in &current_files {
            match self.snapshots.get(path) {
                None => {
                    if let Some(evt) = make_file_event(FileEventType::Created, path, snap.size, now) {
                        new_event_list.push(evt);
                    }
                }
                Some(old) => {
                    if snap.modified != old.modified || snap.size != old.size {
                        if let Some(evt) = make_file_event(FileEventType::Modified, path, snap.size, now) {
                            new_event_list.push(evt);
                        }
                    }
                }
            }
        }

        // Detect deleted files
        for (path, snap) in &self.snapshots {
            if !current_files.contains_key(path) {
                if let Some(evt) = make_file_event(FileEventType::Deleted, path, snap.size, now) {
                    new_event_list.push(evt);
                }
            }
        }

        let new_events = new_event_list.len();
        for evt in new_event_list {
            self.push_event(evt);
        }

        self.snapshots = current_files;
        new_events
    }

    /// Drain and return all accumulated events.
    pub fn drain_events(&mut self) -> Vec<FileEvent> {
        std::mem::take(&mut self.events)
    }

    /// Scan a directory recursively.
    fn scan_dir(dir: &Path, out: &mut HashMap<PathBuf, FileSnapshot>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(meta) = std::fs::metadata(&path) {
                    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    out.insert(
                        path,
                        FileSnapshot {
                            size: meta.len(),
                            modified,
                        },
                    );
                }
            } else if path.is_dir() {
                Self::scan_dir(&path, out);
            }
        }
    }

    fn push_event(&mut self, evt: FileEvent) {
        if self.events.len() >= self.max_buffer {
            self.events.remove(0);
        }
        self.events.push(evt);
    }
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Get drive label and type for a given path.
#[cfg(target_os = "windows")]
fn get_drive_info(path: &Path) -> (String, String) {
    let path_str = path.to_string_lossy();
    let drive_letter = if path_str.len() >= 2 && path_str.as_bytes()[1] == b':' {
        format!("{}:", &path_str[..1])
    } else {
        "?:".into()
    };

    // Determine drive type via GetDriveTypeW
    let wide: Vec<u16> = format!("{}\\", drive_letter)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    let drive_type = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide.as_ptr())
    };

    let type_str = match drive_type {
        2 => "removable",
        3 => "fixed",
        4 => "network",
        5 => "cdrom",
        6 => "ramdisk",
        _ => "unknown",
    };

    (drive_letter, type_str.into())
}

#[cfg(not(target_os = "windows"))]
fn get_drive_info(path: &Path) -> (String, String) {
    // On Linux, check /proc/mounts to determine mount type
    let path_str = path.to_string_lossy();

    let mount_info = std::fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut best_match = ("", "unknown");
    let mut best_len = 0;

    for line in mount_info.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            let mount_point = parts[1];
            if path_str.starts_with(mount_point) && mount_point.len() > best_len {
                best_len = mount_point.len();
                let fs_type = parts[2];
                let drive_type = if mount_point.starts_with("/media")
                    || mount_point.starts_with("/mnt")
                    || fs_type == "vfat"
                    || fs_type == "ntfs"
                    || fs_type == "exfat"
                {
                    "removable"
                } else {
                    "fixed"
                };
                best_match = (mount_point, drive_type);
            }
        }
    }

    (best_match.0.to_string(), best_match.1.to_string())
}

/// Build a [`FileEvent`] from the given parameters (free function to avoid borrow conflicts).
fn make_file_event(
    event_type: FileEventType,
    path: &Path,
    size: u64,
    timestamp: u64,
) -> Option<FileEvent> {
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path_str = path.to_string_lossy().into_owned();
    let (drive_label, drive_type) = get_drive_info(path);

    Some(FileEvent {
        event_type,
        path: path_str,
        filename,
        size_bytes: size,
        drive_label,
        drive_type,
        timestamp,
    })
}

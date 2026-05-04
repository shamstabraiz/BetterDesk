//! Sync history — persistent ring buffer of inventory snapshots.
//!
//! Stores the last N snapshots in a JSON file alongside the main config.
//! Each snapshot records the sync phase, collected data and human-readable
//! change descriptions produced by the diff engine.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;

use super::hardware::HardwareInfo;
use super::software::SoftwareList;

// ---------------------------------------------------------------------------
//  Constants
// ---------------------------------------------------------------------------

/// Maximum number of snapshots kept in the ring buffer.
const MAX_SNAPSHOTS: usize = 200;

/// History filename (stored next to config.json).
const HISTORY_FILE: &str = "sync_history.json";

// ---------------------------------------------------------------------------
//  Public types
// ---------------------------------------------------------------------------

/// A single point-in-time inventory snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncSnapshot {
    /// ISO-8601 timestamp.
    pub timestamp: String,
    /// Sync phase that produced this snapshot (e.g. "lightweight", "storage").
    pub phase: String,
    /// Partial or full hardware info (depending on phase).
    pub hardware: Option<HardwareInfo>,
    /// Software list (only present after the Software phase).
    pub software: Option<SoftwareList>,
    /// Human-readable descriptions of changes since the previous snapshot.
    pub changes: Vec<String>,
}

/// Persistent ring buffer of sync snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncHistory {
    pub snapshots: VecDeque<SyncSnapshot>,
}

// ---------------------------------------------------------------------------
//  Implementation
// ---------------------------------------------------------------------------

impl SyncHistory {
    /// Create an empty history.
    pub fn new() -> Self {
        Self {
            snapshots: VecDeque::new(),
        }
    }

    /// Load history from disk, returning an empty history on any error.
    pub fn load() -> Self {
        match try_load() {
            Ok(h) => h,
            Err(e) => {
                log::debug!("No existing sync history ({}), starting fresh", e);
                Self::new()
            }
        }
    }

    /// Append a snapshot, rotating old entries if the buffer exceeds MAX_SNAPSHOTS.
    pub fn append(&mut self, snapshot: SyncSnapshot) {
        self.snapshots.push_back(snapshot);
        while self.snapshots.len() > MAX_SNAPSHOTS {
            self.snapshots.pop_front();
        }
    }

    /// Return the N most recent snapshots (newest last).
    pub fn recent(&self, n: usize) -> Vec<&SyncSnapshot> {
        let len = self.snapshots.len();
        let start = len.saturating_sub(n);
        self.snapshots.range(start..).collect()
    }

    /// Return the latest snapshot, if any.
    pub fn latest(&self) -> Option<&SyncSnapshot> {
        self.snapshots.back()
    }

    /// Persist history to disk.
    pub fn save(&self) -> Result<()> {
        let path = history_path()?;
        let data = serde_json::to_string(self)
            .context("Failed to serialize sync history")?;
        fs::write(&path, data)
            .with_context(|| format!("Failed to write sync history: {}", path.display()))?;
        log::debug!("Sync history saved ({} snapshots)", self.snapshots.len());
        Ok(())
    }

    /// Number of stored snapshots.
    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}

// ---------------------------------------------------------------------------
//  Internal helpers
// ---------------------------------------------------------------------------

fn history_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("com", "Yomie", "Yomie")
        .context("Cannot determine config directory")?;
    let config_dir = dirs.config_dir();
    fs::create_dir_all(config_dir)
        .with_context(|| format!("Cannot create config dir: {}", config_dir.display()))?;
    Ok(config_dir.join(HISTORY_FILE))
}

fn try_load() -> Result<SyncHistory> {
    let path = history_path()?;
    let data = fs::read_to_string(&path)
        .with_context(|| format!("Cannot read: {}", path.display()))?;
    let history: SyncHistory = serde_json::from_str(&data)
        .with_context(|| format!("Cannot parse: {}", path.display()))?;
    Ok(history)
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_rotation() {
        let mut h = SyncHistory::new();
        for i in 0..(MAX_SNAPSHOTS + 10) {
            h.append(SyncSnapshot {
                timestamp: format!("2026-01-01T00:00:{:02}Z", i % 60),
                phase: "test".into(),
                hardware: None,
                software: None,
                changes: vec![format!("change {}", i)],
            });
        }
        assert_eq!(h.len(), MAX_SNAPSHOTS);
        // Oldest entry should be the 10th (index 10) since first 10 were evicted.
        let first = h.snapshots.front().unwrap();
        assert!(first.changes[0].contains("10"));
    }

    #[test]
    fn test_recent() {
        let mut h = SyncHistory::new();
        for i in 0..5 {
            h.append(SyncSnapshot {
                timestamp: String::new(),
                phase: format!("p{}", i),
                hardware: None,
                software: None,
                changes: vec![],
            });
        }
        let recent = h.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].phase, "p2");
        assert_eq!(recent[2].phase, "p4");
    }
}

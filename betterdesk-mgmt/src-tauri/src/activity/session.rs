//! Yomie Activity Session — aggregation logic
//!
//! Collects foreground snapshots into contiguous sessions:
//! when the same app + window stays in focus, the session duration grows.
//! When the foreground changes, the previous session is finalized.

use serde::{Deserialize, Serialize};
use super::tracker::ForegroundSnapshot;

/// A contiguous period of focus on one application/window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySession {
    pub app_name: String,
    pub window_title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_secs: u64,
    pub category: Option<String>,
}

/// Aggregator that converts raw foreground snapshots into sessions.
pub struct SessionAggregator {
    current: Option<ActiveEntry>,
    completed: Vec<ActivitySession>,
    /// Maximum sessions kept in memory before flushing.
    max_buffer: usize,
}

struct ActiveEntry {
    app_name: String,
    window_title: String,
    started_at: String,
    last_seen: std::time::Instant,
}

impl SessionAggregator {
    pub fn new() -> Self {
        Self {
            current: None,
            completed: Vec::new(),
            max_buffer: 500,
        }
    }

    /// Feed a new foreground snapshot. If the app/window changed,
    /// the previous session is finalized and pushed to completed.
    pub fn feed(&mut self, snap: &ForegroundSnapshot) {
        let now = std::time::Instant::now();

        match &self.current {
            Some(entry) if entry.app_name == snap.app_name && entry.window_title == snap.window_title => {
                // Same app/window — update timestamp
                if let Some(ref mut e) = self.current {
                    e.last_seen = now;
                }
            }
            _ => {
                // Finalize the old session if any
                if let Some(old) = self.current.take() {
                    let _duration = old.last_seen.elapsed().as_secs().max(0)
                        + (now - old.last_seen).as_secs();
                    // Duration is from start to this moment
                    let total_duration = (now - std::time::Instant::now()).as_secs()
                        .max(0);
                    let _ = total_duration; // Not used — we compute from timestamps

                    let session = ActivitySession {
                        app_name: old.app_name,
                        window_title: old.window_title,
                        started_at: old.started_at,
                        ended_at: Some(snap.timestamp.clone()),
                        duration_secs: old.last_seen.duration_since(
                            // approximate duration
                            std::time::Instant::now() - old.last_seen.elapsed()
                        ).as_secs().max(1),
                        category: None,
                    };
                    self.completed.push(session);

                    // Prevent unbounded growth
                    if self.completed.len() > self.max_buffer {
                        self.completed.drain(0..100);
                    }
                }

                // Start a new session
                self.current = Some(ActiveEntry {
                    app_name: snap.app_name.clone(),
                    window_title: snap.window_title.clone(),
                    started_at: snap.timestamp.clone(),
                    last_seen: now,
                });
            }
        }
    }

    /// Drain all completed sessions (for upload).
    pub fn drain_completed(&mut self) -> Vec<ActivitySession> {
        std::mem::take(&mut self.completed)
    }

    /// Get the number of completed sessions waiting to be uploaded.
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.completed.len()
    }

    /// Get current active session info (if any).
    #[allow(dead_code)]
    pub fn current_app(&self) -> Option<String> {
        self.current.as_ref().map(|e| e.app_name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_aggregation() {
        let mut agg = SessionAggregator::new();

        let snap1 = ForegroundSnapshot {
            app_name: "chrome.exe".into(),
            window_title: "Google".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
        };
        agg.feed(&snap1);
        assert_eq!(agg.pending_count(), 0); // First feed, no completed yet
        assert_eq!(agg.current_app(), Some("chrome.exe".into()));

        let snap2 = ForegroundSnapshot {
            app_name: "notepad.exe".into(),
            window_title: "Untitled".into(),
            timestamp: "2026-01-01T00:00:05Z".into(),
        };
        agg.feed(&snap2);
        assert_eq!(agg.pending_count(), 1); // chrome session completed
        assert_eq!(agg.current_app(), Some("notepad.exe".into()));
    }
}

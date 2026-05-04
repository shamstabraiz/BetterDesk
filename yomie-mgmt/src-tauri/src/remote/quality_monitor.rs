//! Connection quality monitoring — tracks FPS, latency, bandwidth,
//! and frame loss during a remote desktop session.

use log::debug;
use std::collections::VecDeque;
use std::time::Instant;

use crate::proto::TestDelay;

/// Rolling window size for bandwidth calculation.
const BANDWIDTH_WINDOW_SECS: f64 = 5.0;

/// Quality monitor for a remote session.
pub struct QualityMonitor {
    last_latency_ms: u32,
    frame_sizes: VecDeque<(Instant, usize)>,
    total_bytes: u64,
}

impl QualityMonitor {
    pub fn new() -> Self {
        Self {
            last_latency_ms: 0,
            frame_sizes: VecDeque::with_capacity(300),
            total_bytes: 0,
        }
    }

    /// Record a received video frame for bandwidth calculation.
    pub fn record_frame(&mut self, bytes: usize) {
        let now = Instant::now();
        self.frame_sizes.push_back((now, bytes));
        self.total_bytes += bytes as u64;

        // Prune old entries
        let cutoff = now - std::time::Duration::from_secs(BANDWIDTH_WINDOW_SECS as u64);
        while self.frame_sizes.front().map_or(false, |(t, _)| *t < cutoff) {
            self.frame_sizes.pop_front();
        }
    }

    /// Handle a TestDelay round-trip from the peer.
    pub fn handle_test_delay(&mut self, td: &TestDelay) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let rtt = (now_ms - td.time).max(0) as u32;
        self.last_latency_ms = rtt;
        debug!("QualityMonitor: latency={}ms", rtt);
    }

    /// Current latency in ms.
    pub fn latency_ms(&self) -> u32 {
        self.last_latency_ms
    }

    /// Current bandwidth in kbps.
    pub fn bandwidth_kbps(&self) -> f64 {
        if self.frame_sizes.len() < 2 {
            return 0.0;
        }

        let first = self.frame_sizes.front().unwrap().0;
        let last = self.frame_sizes.back().unwrap().0;
        let elapsed = last.duration_since(first).as_secs_f64();

        if elapsed < 0.1 {
            return 0.0;
        }

        let total_bytes: usize = self.frame_sizes.iter().map(|(_, s)| *s).sum();
        (total_bytes as f64 * 8.0) / (elapsed * 1000.0) // kbps
    }
}

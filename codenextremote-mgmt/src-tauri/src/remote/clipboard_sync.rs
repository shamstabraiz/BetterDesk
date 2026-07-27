//! Bidirectional clipboard synchronization during a remote session.
//!
//! Uses the `arboard` crate for local clipboard access.
//! Sends/receives `Clipboard` protobuf messages via the relay.

use anyhow::Result;
use log::{debug, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::proto::{
    Clipboard as ClipboardMsg, ClipboardFormat,
    message::Union as MsgUnion, Message as PeerMessage,
};

/// Minimum interval between outgoing clipboard messages.
const SYNC_COOLDOWN: Duration = Duration::from_millis(500);

/// Maximum clipboard content size (2 MB).
const MAX_CLIPBOARD_SIZE: usize = 2 * 1024 * 1024;

/// Clipboard sync state.
pub struct ClipboardSync {
    enabled: Arc<AtomicBool>,
    last_sent_hash: u64,
    last_send_time: Instant,
    last_received_hash: u64,
}

impl ClipboardSync {
    /// Create a new clipboard sync (disabled by default).
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(false)),
            last_sent_hash: 0,
            last_send_time: Instant::now() - SYNC_COOLDOWN,
            last_received_hash: 0,
        }
    }

    /// Enable or disable clipboard sync.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
        info!("ClipboardSync: {}", if enabled { "enabled" } else { "disabled" });
    }

    /// Whether clipboard sync is currently enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Check local clipboard and return a message if content changed.
    ///
    /// Call this periodically (e.g. every 500ms) from the session loop.
    pub fn poll_local(&mut self) -> Option<PeerMessage> {
        if !self.is_enabled() {
            return None;
        }

        // Throttle
        if self.last_send_time.elapsed() < SYNC_COOLDOWN {
            return None;
        }

        let text = match crate::clipboard::get_text() {
            Ok(t) => t,
            Err(_) => return None,
        };

        if text.is_empty() || text.len() > MAX_CLIPBOARD_SIZE {
            return None;
        }

        let hash = simple_hash(text.as_bytes());

        // Skip if same content we last sent or last received from peer
        if hash == self.last_sent_hash || hash == self.last_received_hash {
            return None;
        }

        self.last_sent_hash = hash;
        self.last_send_time = Instant::now();

        debug!("ClipboardSync: sending {} bytes to peer", text.len());

        let msg = PeerMessage {
            union: Some(MsgUnion::Clipboard(ClipboardMsg {
                compress: false,
                content: text.into_bytes(),
                width: 0,
                height: 0,
                format: ClipboardFormat::Text as i32,
                special_name: String::new(),
            })),
        };

        Some(msg)
    }

    /// Handle a clipboard message received from the peer.
    pub fn handle_remote(&mut self, clipboard: &ClipboardMsg) -> Result<()> {
        if !self.is_enabled() {
            debug!("ClipboardSync: received clipboard but sync disabled");
            return Ok(());
        }

        if clipboard.content.is_empty() {
            return Ok(());
        }

        if clipboard.content.len() > MAX_CLIPBOARD_SIZE {
            warn!(
                "ClipboardSync: ignoring oversized clipboard ({} bytes)",
                clipboard.content.len()
            );
            return Ok(());
        }

        let text = String::from_utf8_lossy(&clipboard.content).to_string();
        let hash = simple_hash(text.as_bytes());

        // Skip if same as what we already have
        if hash == self.last_received_hash {
            return Ok(());
        }

        self.last_received_hash = hash;

        debug!("ClipboardSync: received {} bytes from peer", text.len());
        crate::clipboard::set_text(&text)?;

        Ok(())
    }
}

/// Simple non-cryptographic hash for change detection.
fn simple_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 5381;
    for &b in data {
        hash = hash.wrapping_mul(33).wrapping_add(b as u64);
    }
    hash
}

//! Session recording — records H.264 NALUs and input events to a binary file
//! for later playback and audit review.
//!
//! File format:
//! ```text
//! [4 bytes] magic "BDRC"
//! [4 bytes] version (1)
//! [8 bytes] session start timestamp (ms since epoch)
//! Records:
//!   [8 bytes] timestamp_ms (relative to start)
//!   [1 byte ] record_type (0=video, 1=mouse, 2=key, 3=cursor)
//!   [4 bytes] data_length (LE u32)
//!   [N bytes] data
//! ```

use anyhow::{Context, Result};
use log::{info, warn};
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

const MAGIC: &[u8; 4] = b"BDRC";
const VERSION: u32 = 1;

/// Record type identifiers.
const RECORD_VIDEO: u8 = 0;
const RECORD_MOUSE: u8 = 1;
const RECORD_KEY: u8 = 2;
const RECORD_CURSOR: u8 = 3;

/// Session recorder.
pub struct SessionRecorder {
    file: Option<std::fs::File>,
    path: PathBuf,
    start_time: Instant,
    start_epoch_ms: u64,
    bytes_written: u64,
    record_count: u64,
}

impl SessionRecorder {
    /// Create a new recorder.  Does not open the file until `start()` is called.
    pub fn new() -> Self {
        let recordings_dir = dirs::document_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .join("Yomie")
            .join("recordings");

        let _ = std::fs::create_dir_all(&recordings_dir);

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let path = recordings_dir.join(format!("session_{}.bdrc", timestamp));

        Self {
            file: None,
            path,
            start_time: Instant::now(),
            start_epoch_ms: 0,
            bytes_written: 0,
            record_count: 0,
        }
    }

    /// Start recording.
    pub fn start(&mut self) -> Result<PathBuf> {
        let mut file = std::fs::File::create(&self.path)
            .context("Failed to create recording file")?;

        self.start_time = Instant::now();
        self.start_epoch_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        // Write header
        file.write_all(MAGIC)?;
        file.write_all(&VERSION.to_le_bytes())?;
        file.write_all(&self.start_epoch_ms.to_le_bytes())?;

        self.file = Some(file);
        self.bytes_written = 16; // header size
        self.record_count = 0;

        info!("SessionRecorder: started → {}", self.path.display());
        Ok(self.path.clone())
    }

    /// Stop recording and finalize the file.
    pub fn stop(&mut self) {
        if let Some(mut file) = self.file.take() {
            let _ = file.flush();
            info!(
                "SessionRecorder: stopped — {} records, {} bytes",
                self.record_count, self.bytes_written
            );
        }
    }

    /// Whether the recorder is active.
    pub fn is_recording(&self) -> bool {
        self.file.is_some()
    }

    /// Record a video NALU.
    pub fn record_video(&mut self, data: &[u8]) {
        self.write_record(RECORD_VIDEO, data);
    }

    /// Record a mouse event (serialized as JSON bytes).
    pub fn record_mouse(&mut self, data: &[u8]) {
        self.write_record(RECORD_MOUSE, data);
    }

    /// Record a key event (serialized as JSON bytes).
    pub fn record_key(&mut self, data: &[u8]) {
        self.write_record(RECORD_KEY, data);
    }

    /// Record cursor data.
    pub fn record_cursor(&mut self, data: &[u8]) {
        self.write_record(RECORD_CURSOR, data);
    }

    /// Path to the recording file.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    // -----------------------------------------------------------------------
    //  Internal
    // -----------------------------------------------------------------------

    fn write_record(&mut self, record_type: u8, data: &[u8]) {
        let file = match self.file.as_mut() {
            Some(f) => f,
            None => return,
        };

        let ts = self.start_time.elapsed().as_millis() as u64;
        let len = data.len() as u32;

        // [8 bytes ts] [1 byte type] [4 bytes len] [data]
        let header_size = 8 + 1 + 4;
        let mut buf = Vec::with_capacity(header_size + data.len());
        buf.extend_from_slice(&ts.to_le_bytes());
        buf.push(record_type);
        buf.extend_from_slice(&len.to_le_bytes());
        buf.extend_from_slice(data);

        if let Err(e) = file.write_all(&buf) {
            warn!("SessionRecorder: write error: {}", e);
            return;
        }

        self.bytes_written += buf.len() as u64;
        self.record_count += 1;
    }
}

impl Drop for SessionRecorder {
    fn drop(&mut self) {
        self.stop();
    }
}

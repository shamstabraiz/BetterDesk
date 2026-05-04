//! Video receive pipeline — decodes protobuf `VideoFrame` messages from the
//! relay connection and emits decoded RGBA frames to the frontend.
//!
//! Supports H.264 via `openh264`.  Other codec variants (VP9, VP8, AV1) are
//! logged and skipped — the peer negotiates H.264 when we advertise support.

use anyhow::{Context, Result};
use log::{debug, trace, warn};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::codec::{DecodedFrame, H264Decoder};
use crate::proto::{EncodedVideoFrames, VideoFrame, video_frame};

// ---------------------------------------------------------------------------
//  Public types
// ---------------------------------------------------------------------------

/// Statistics about the video pipeline.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VideoStats {
    pub frames_decoded: u64,
    pub frames_dropped: u64,
    pub bytes_received: u64,
    pub fps: f32,
    pub width: u32,
    pub height: u32,
    pub codec: String,
}

// ---------------------------------------------------------------------------
//  VideoPipeline
// ---------------------------------------------------------------------------

/// Stateful video pipeline that decodes `VideoFrame` protobuf messages.
pub struct VideoPipeline {
    decoder: H264Decoder,
    frames_decoded: Arc<AtomicU64>,
    frames_dropped: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    last_fps_time: Instant,
    last_fps_count: u64,
    current_fps: f32,
    last_width: u32,
    last_height: u32,
    active_codec: String,
}

impl VideoPipeline {
    /// Create a new video pipeline.
    pub fn new() -> Result<Self> {
        let decoder = H264Decoder::new().context("Failed to init H.264 decoder")?;
        Ok(Self {
            decoder,
            frames_decoded: Arc::new(AtomicU64::new(0)),
            frames_dropped: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            last_fps_time: Instant::now(),
            last_fps_count: 0,
            current_fps: 0.0,
            last_width: 0,
            last_height: 0,
            active_codec: String::new(),
        })
    }

    /// Process a `VideoFrame` protobuf message.
    ///
    /// Returns decoded RGBA frames (zero or more — a single VideoFrame may
    /// contain multiple NALUs).
    pub fn process_video_frame(&mut self, vf: &VideoFrame) -> Vec<DecodedFrame> {
        let mut decoded = Vec::new();

        match &vf.union {
            Some(video_frame::Union::H264s(frames)) => {
                self.active_codec = "h264".into();
                self.decode_encoded_frames(frames, &mut decoded);
            }
            Some(video_frame::Union::Vp9s(frames)) => {
                // VP9 decode not implemented — count as dropped
                self.active_codec = "vp9".into();
                let count = frames.frames.len() as u64;
                self.frames_dropped.fetch_add(count, Ordering::Relaxed);
                trace!("VP9 frames skipped: {}", count);
            }
            Some(video_frame::Union::Vp8s(frames)) => {
                self.active_codec = "vp8".into();
                let count = frames.frames.len() as u64;
                self.frames_dropped.fetch_add(count, Ordering::Relaxed);
                trace!("VP8 frames skipped: {}", count);
            }
            Some(video_frame::Union::H265s(frames)) => {
                self.active_codec = "h265".into();
                let count = frames.frames.len() as u64;
                self.frames_dropped.fetch_add(count, Ordering::Relaxed);
                trace!("H265 frames skipped: {}", count);
            }
            Some(video_frame::Union::Av1s(frames)) => {
                self.active_codec = "av1".into();
                let count = frames.frames.len() as u64;
                self.frames_dropped.fetch_add(count, Ordering::Relaxed);
                trace!("AV1 frames skipped: {}", count);
            }
            Some(video_frame::Union::Rgb(_)) | Some(video_frame::Union::Yuv(_)) => {
                // Raw pixel data — not commonly used in relay
                self.active_codec = "raw".into();
                debug!("Raw RGB/YUV frame received — not decoded");
            }
            None => {
                warn!("VideoFrame with no codec union");
            }
        }

        decoded
    }

    /// Get current statistics snapshot.
    pub fn stats(&mut self) -> VideoStats {
        self.update_fps();
        VideoStats {
            frames_decoded: self.frames_decoded.load(Ordering::Relaxed),
            frames_dropped: self.frames_dropped.load(Ordering::Relaxed),
            bytes_received: self.bytes_received.load(Ordering::Relaxed),
            fps: self.current_fps,
            width: self.last_width,
            height: self.last_height,
            codec: self.active_codec.clone(),
        }
    }

    // -----------------------------------------------------------------------
    //  Internal
    // -----------------------------------------------------------------------

    fn decode_encoded_frames(
        &mut self,
        frames: &EncodedVideoFrames,
        out: &mut Vec<DecodedFrame>,
    ) {
        for ef in &frames.frames {
            self.bytes_received
                .fetch_add(ef.data.len() as u64, Ordering::Relaxed);

            match self.decoder.decode(&ef.data) {
                Ok(Some(frame)) => {
                    self.last_width = frame.width;
                    self.last_height = frame.height;
                    self.frames_decoded.fetch_add(1, Ordering::Relaxed);
                    out.push(frame);
                }
                Ok(None) => {
                    // Decoder buffered data (SPS/PPS) — no picture yet
                    trace!("H264: no picture from {} bytes (key={})", ef.data.len(), ef.key);
                }
                Err(e) => {
                    self.frames_dropped.fetch_add(1, Ordering::Relaxed);
                    debug!("H264 decode error: {} ({} bytes, key={})", e, ef.data.len(), ef.key);
                }
            }
        }
    }

    fn update_fps(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_fps_time).as_secs_f32();
        if elapsed >= 1.0 {
            let current = self.frames_decoded.load(Ordering::Relaxed);
            let delta = current - self.last_fps_count;
            self.current_fps = delta as f32 / elapsed;
            self.last_fps_count = current;
            self.last_fps_time = now;
        }
    }
}

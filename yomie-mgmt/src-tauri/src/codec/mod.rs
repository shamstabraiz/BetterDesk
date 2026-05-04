//! Video codec — encoding and decoding for remote desktop streaming.
//!
//! Supported codecs:
//! - H.264 (via openh264 — software decoder, no external deps)
//! - Raw BGRA (fallback, no compression)
//!
//! VP9/AV1 are declared for protocol negotiation but decode is not
//! implemented; H.264 is the primary codec used by RustDesk peers.

use anyhow::{Context, Result};
use log::{debug, info, warn};
use openh264::formats::YUVSource;
use std::sync::Mutex;

/// Supported video codecs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VideoCodec {
    Raw,
    Vp9,
    H264,
    H265,
    Vp8,
    Av1,
}

impl std::fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VideoCodec::Raw => write!(f, "raw"),
            VideoCodec::Vp9 => write!(f, "vp9"),
            VideoCodec::H264 => write!(f, "h264"),
            VideoCodec::H265 => write!(f, "h265"),
            VideoCodec::Vp8 => write!(f, "vp8"),
            VideoCodec::Av1 => write!(f, "av1"),
        }
    }
}

// ---------------------------------------------------------------------------
//  H.264 Decoder (openh264)
// ---------------------------------------------------------------------------

/// Stateful H.264 decoder wrapping `openh264::decoder::Decoder`.
///
/// Call `decode()` with a single NALU or Annex-B byte stream.
/// Returns RGBA pixel data + (width, height).
pub struct H264Decoder {
    inner: Mutex<openh264::decoder::Decoder>,
}

/// Decoded video frame — RGBA pixels.
pub struct DecodedFrame {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl H264Decoder {
    /// Create a new H.264 decoder instance.
    pub fn new() -> Result<Self> {
        let decoder = openh264::decoder::Decoder::new()
            .context("Failed to create openh264 decoder")?;
        info!("H264Decoder: openh264 decoder initialized");
        Ok(Self {
            inner: Mutex::new(decoder),
        })
    }

    /// Decode one or more NALUs.
    ///
    /// `data` should be Annex-B formatted (0x00 0x00 0x00 0x01 prefix)
    /// or a single NALU.  Returns `None` if the decoder needs more data
    /// (e.g. SPS/PPS only, no picture yet).
    pub fn decode(&self, data: &[u8]) -> Result<Option<DecodedFrame>> {
        if data.is_empty() {
            return Ok(None);
        }

        let mut dec = self.inner.lock().map_err(|e| {
            anyhow::anyhow!("H264Decoder lock poisoned: {}", e)
        })?;

        match dec.decode(data) {
            Ok(Some(yuv)) => {
                let (w, h) = yuv.dimensions();
                let mut rgba = vec![0u8; w * h * 4];
                // Convert YUV420 → RGB, then pack into RGBA
                let mut rgb = vec![0u8; w * h * 3];
                yuv.write_rgb8(&mut rgb);
                for i in 0..(w * h) {
                    rgba[i * 4] = rgb[i * 3];
                    rgba[i * 4 + 1] = rgb[i * 3 + 1];
                    rgba[i * 4 + 2] = rgb[i * 3 + 2];
                    rgba[i * 4 + 3] = 255;
                }
                Ok(Some(DecodedFrame {
                    rgba,
                    width: w as u32,
                    height: h as u32,
                }))
            }
            Ok(None) => {
                // Decoder consumed data but produced no picture yet
                // (SPS/PPS parameter sets, or B-frame buffering)
                Ok(None)
            }
            Err(e) => {
                warn!("H264Decoder: decode error: {}", e);
                Err(anyhow::anyhow!("H.264 decode failed: {}", e))
            }
        }
    }
}

// ---------------------------------------------------------------------------
//  Stateless helpers (kept for backward compatibility)
// ---------------------------------------------------------------------------

/// Decode a compressed video frame (stateless — creates a temporary decoder).
///
/// For streaming use, prefer `H264Decoder` directly.
pub fn decode_frame(
    codec: VideoCodec,
    data: &[u8],
    _width: u32,
    _height: u32,
) -> Result<Vec<u8>> {
    debug!("Decode frame: codec={}, {} bytes", codec, data.len());

    match codec {
        VideoCodec::Raw => Ok(data.to_vec()),
        VideoCodec::H264 => {
            let dec = H264Decoder::new()?;
            match dec.decode(data)? {
                Some(frame) => Ok(frame.rgba),
                None => Err(anyhow::anyhow!("No picture produced from single frame")),
            }
        }
        _ => Err(anyhow::anyhow!(
            "Codec {} decoding not yet implemented",
            codec
        )),
    }
}

/// Encode a raw BGRA frame.
pub fn encode_frame(
    codec: VideoCodec,
    data: &[u8],
    _width: u32,
    _height: u32,
) -> Result<Vec<u8>> {
    debug!("Encode frame: codec={}, {} bytes", codec, data.len());

    match codec {
        VideoCodec::Raw => Ok(data.to_vec()),
        _ => Err(anyhow::anyhow!(
            "Codec {} encoding not yet implemented",
            codec
        )),
    }
}

/// Negotiate the best codec supported by both sides.
pub fn negotiate_codec(
    local_supported: &[VideoCodec],
    remote_supported: &[VideoCodec],
) -> VideoCodec {
    // Preference order: H264 > VP9 > VP8 > Raw
    // H264 is the only codec we can actually decode.
    let preference = [
        VideoCodec::H264,
        VideoCodec::Vp9,
        VideoCodec::Vp8,
        VideoCodec::Raw,
    ];

    for codec in &preference {
        if local_supported.contains(codec) && remote_supported.contains(codec) {
            return *codec;
        }
    }

    VideoCodec::Raw
}

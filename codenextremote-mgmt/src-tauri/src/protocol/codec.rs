//! RustDesk custom BytesCodec framing.
//!
//! Wire format (matches `hbb_common::bytes_codec::BytesCodec`):
//!
//! The lower 2 bits of the first byte encode the header length:
//!
//! | byte[0] & 0x3 | Header size | Max payload       |
//! |---------------|-------------|-------------------|
//! | 0 (0b00)      | 1 byte      | 63 B              |
//! | 1 (0b01)      | 2 bytes LE  | 16 383 B          |
//! | 2 (0b10)      | 3 bytes LE  | ~4 MB             |
//! | 3 (0b11)      | 4 bytes LE  | ~1 GB             |
//!
//! Header value = `(payload_len << 2) | size_tag`, stored little-endian.
//! Payload follows immediately after the header.

use anyhow::{bail, Context, Result};
use bytes::{Buf, BytesMut};
use prost::Message;
use tokio::io::AsyncReadExt;

/// Maximum single frame size (16 MiB — generous for video frames).
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Encode a protobuf message into a RustDesk-framed byte buffer.
pub fn write_frame<M: Message>(msg: &M) -> Vec<u8> {
    let payload = msg.encode_to_vec();
    encode_frame(&payload)
}

/// Encode raw payload bytes into a RustDesk-framed byte buffer.
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let len = payload.len();
    let mut buf = Vec::with_capacity(4 + len);

    if len <= 0x3F {
        // 1-byte header
        buf.push((len << 2) as u8);
    } else if len <= 0x3FFF {
        // 2-byte header (LE u16)
        let h = ((len << 2) as u16) | 0x1;
        buf.push(h as u8);
        buf.push((h >> 8) as u8);
    } else if len <= 0x3F_FFFF {
        // 3-byte header (LE u24)
        let h = ((len << 2) as u32) | 0x2;
        buf.push(h as u8);
        buf.push((h >> 8) as u8);
        buf.push((h >> 16) as u8);
    } else {
        // 4-byte header (LE u32)
        let h = ((len << 2) as u32) | 0x3;
        buf.push(h as u8);
        buf.push((h >> 8) as u8);
        buf.push((h >> 16) as u8);
        buf.push((h >> 24) as u8);
    }

    buf.extend_from_slice(payload);
    buf
}

/// Read a single RustDesk-framed message from an async reader.
///
/// Returns the raw payload bytes (without the header).
pub async fn read_frame<R: AsyncReadExt + Unpin>(reader: &mut R) -> Result<Vec<u8>> {
    // Read the first byte to determine header length
    let mut first = [0u8; 1];
    reader
        .read_exact(&mut first)
        .await
        .context("Connection closed — failed to read frame")?;

    let tag = first[0] & 0x3;
    let head_len = (tag + 1) as usize; // 1, 2, 3, or 4

    // Read remaining header bytes (if any)
    let mut hdr_buf = [0u8; 4];
    hdr_buf[0] = first[0];
    if head_len > 1 {
        reader
            .read_exact(&mut hdr_buf[1..head_len])
            .await
            .context("Failed to read frame header")?;
    }

    // Assemble the header value (little-endian)
    let mut raw: u32 = 0;
    for i in 0..head_len {
        raw |= (hdr_buf[i] as u32) << (i * 8);
    }

    // Payload length = header >> 2
    let length = (raw >> 2) as usize;

    if length == 0 {
        bail!("Received zero-length frame");
    }
    if length > MAX_FRAME_SIZE {
        bail!("Frame too large: {} bytes (max {})", length, MAX_FRAME_SIZE);
    }

    let mut buf = vec![0u8; length];
    reader
        .read_exact(&mut buf)
        .await
        .context("Failed to read frame payload")?;
    Ok(buf)
}

/// Streaming frame codec with internal buffer for partial reads.
pub struct FrameCodec {
    buf: BytesMut,
}

impl FrameCodec {
    pub fn new() -> Self {
        Self {
            buf: BytesMut::with_capacity(8192),
        }
    }

    /// Feed raw bytes into the codec buffer.
    pub fn feed(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Try to extract the next complete frame from the buffer.
    ///
    /// Returns `None` if not enough data is available yet.
    pub fn try_decode(&mut self) -> Result<Option<Vec<u8>>> {
        if self.buf.is_empty() {
            return Ok(None);
        }

        let first = self.buf[0];
        let tag = first & 0x3;
        let head_len = (tag + 1) as usize;

        if self.buf.len() < head_len {
            return Ok(None); // Need more header bytes
        }

        // Assemble the header value (little-endian)
        let mut raw: u32 = 0;
        for i in 0..head_len {
            raw |= (self.buf[i] as u32) << (i * 8);
        }
        let length = (raw >> 2) as usize;

        if length > MAX_FRAME_SIZE {
            bail!(
                "Frame too large: {} bytes (max {})",
                length,
                MAX_FRAME_SIZE
            );
        }

        let total = head_len + length;
        if self.buf.len() < total {
            return Ok(None); // Need more payload bytes
        }

        // Consume header + payload
        self.buf.advance(head_len);
        let payload = self.buf.split_to(length).to_vec();
        Ok(Some(payload))
    }

    /// Encode a protobuf message into a RustDesk-framed buffer.
    pub fn encode<M: Message>(&self, msg: &M) -> Vec<u8> {
        write_frame(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_round_trip_small() {
        // 10 bytes → 1-byte header
        let payload = b"helloworld";
        let frame = encode_frame(payload);
        assert_eq!(frame[0], (10 << 2) as u8); // 0x28, tag=0
        assert_eq!(&frame[1..], payload.as_slice());
    }

    #[test]
    fn frame_round_trip_medium() {
        // 100 bytes → 2-byte header
        let payload = vec![0xAB; 100];
        let frame = encode_frame(&payload);
        let expected_hdr: u16 = (100 << 2) | 1; // 0x191
        assert_eq!(frame[0], expected_hdr as u8);  // 0x91
        assert_eq!(frame[1], (expected_hdr >> 8) as u8); // 0x01
        assert_eq!(&frame[2..], payload.as_slice());
    }

    #[test]
    fn frame_codec_split_delivery() {
        let mut codec = FrameCodec::new();

        let frame = encode_frame(b"hello");

        // Feed partial data
        codec.feed(&frame[..2]);
        assert!(codec.try_decode().unwrap().is_none());

        // Feed the rest
        codec.feed(&frame[2..]);
        let result = codec.try_decode().unwrap().unwrap();
        assert_eq!(result, b"hello");
    }

    #[test]
    fn frame_codec_multiple_frames() {
        let mut codec = FrameCodec::new();

        let mut data = Vec::new();
        for msg in [b"aaa".as_slice(), b"bbbbb", b"c"] {
            data.extend_from_slice(&encode_frame(msg));
        }

        codec.feed(&data);

        assert_eq!(codec.try_decode().unwrap().unwrap(), b"aaa");
        assert_eq!(codec.try_decode().unwrap().unwrap(), b"bbbbb");
        assert_eq!(codec.try_decode().unwrap().unwrap(), b"c");
        assert!(codec.try_decode().unwrap().is_none());
    }

    #[tokio::test]
    async fn read_write_frame_async() {
        let payload = b"test message";
        let frame = encode_frame(payload);

        let mut cursor = std::io::Cursor::new(frame);
        let result = read_frame(&mut cursor).await.unwrap();
        assert_eq!(result, payload.as_slice());
    }
}

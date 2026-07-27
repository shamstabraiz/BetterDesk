//! Remote desktop — screen capture, input injection, and JPEG frame
//! streaming over CDAP.
//!
//! Uses the `scrap` crate for screen capture and `enigo` for input.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use log::{debug, error, info, warn};
use scrap::{Capturer, Display};
use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use super::connection::CdapSender;
use super::protocol::{CdapMessage, MessagePayload};
use super::SessionManager;

/// Active desktop streaming session.
pub struct DesktopSession {
    running: Arc<AtomicBool>,
    capture_task: tokio::task::JoinHandle<()>,
}

impl DesktopSession {
    /// Stop the desktop session.
    pub async fn stop(self) {
        self.running.store(false, Ordering::Relaxed);
        self.capture_task.abort();
    }
}

/// Start desktop capture streaming.
pub fn start_desktop_session(
    session_id: String,
    fps: u32,
    quality: u8,
    sender: Arc<Mutex<CdapSender>>,
) -> Result<DesktopSession> {
    let running = Arc::new(AtomicBool::new(true));
    let running2 = running.clone();

    let capture_task = tokio::task::spawn_blocking(move || {
        if let Err(e) = capture_loop(session_id, fps, quality, running2, sender) {
            error!("CDAP-desktop: Capture loop error: {}", e);
        }
    });

    Ok(DesktopSession {
        running,
        capture_task,
    })
}

/// Capture → encode JPEG → send frames.
fn capture_loop(
    session_id: String,
    fps: u32,
    quality: u8,
    running: Arc<AtomicBool>,
    sender: Arc<Mutex<CdapSender>>,
) -> Result<()> {
    let display = Display::primary().context("No primary display")?;
    let mut capturer = Capturer::new(display).context("Create capturer")?;

    let width = capturer.width() as u32;
    let height = capturer.height() as u32;
    let frame_interval = Duration::from_millis(1000 / fps.max(1) as u64);

    info!(
        "CDAP-desktop: Streaming {}x{} @ {}fps (quality={})",
        width, height, fps, quality
    );

    let rt = tokio::runtime::Handle::current();

    while running.load(Ordering::Relaxed) {
        let start = std::time::Instant::now();

        match capturer.frame() {
            Ok(frame) => {
                // Convert BGRA → RGB → JPEG
                let rgb = bgra_to_rgb(&frame, width as usize, height as usize);
                let jpeg = encode_jpeg(&rgb, width, height, quality)?;
                let b64 = B64.encode(&jpeg);

                let msg = CdapMessage {
                    msg_type: "desktop_frame".into(),
                    payload: MessagePayload::DesktopFrame {
                        session_id: session_id.clone(),
                        data: b64,
                        format: "jpeg".into(),
                        timestamp: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_millis() as u64)
                            .unwrap_or(0),
                        width,
                        height,
                    },
                };

                let sender2 = sender.clone();
                rt.block_on(async {
                    let mut tx = sender2.lock().await;
                    if let Err(e) = tx.send_message(&msg).await {
                        warn!("CDAP-desktop: Send frame error: {}", e);
                    }
                });
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                // Frame not ready yet
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(e) => {
                error!("CDAP-desktop: Capture error: {}", e);
                std::thread::sleep(Duration::from_millis(100));
            }
        }

        let elapsed = start.elapsed();
        if elapsed < frame_interval {
            std::thread::sleep(frame_interval - elapsed);
        }
    }

    info!("CDAP-desktop: Streaming stopped for {}", session_id);
    Ok(())
}

/// Convert BGRA pixel data to RGB.
fn bgra_to_rgb(bgra: &[u8], width: usize, height: usize) -> Vec<u8> {
    let pixel_count = width * height;
    let mut rgb = Vec::with_capacity(pixel_count * 3);

    // scrap uses stride (row padding), handle it
    let stride = bgra.len() / height;

    for y in 0..height {
        let row_start = y * stride;
        for x in 0..width {
            let offset = row_start + x * 4;
            if offset + 2 < bgra.len() {
                rgb.push(bgra[offset + 2]); // R
                rgb.push(bgra[offset + 1]); // G
                rgb.push(bgra[offset]);     // B
            }
        }
    }

    rgb
}

/// Encode RGB data as JPEG.
fn encode_jpeg(rgb: &[u8], width: u32, height: u32, quality: u8) -> Result<Vec<u8>> {
    use image::codecs::jpeg::JpegEncoder;
    use image::ColorType;
    use std::io::Cursor;

    let mut buf = Cursor::new(Vec::new());
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    encoder
        .encode(rgb, width, height, ColorType::Rgb8.into())
        .context("JPEG encode")?;

    Ok(buf.into_inner())
}

/// Capture a single screenshot as base64 JPEG.
pub fn capture_screenshot_base64() -> Result<String> {
    let display = Display::primary().context("No primary display")?;
    let mut capturer = Capturer::new(display).context("Create capturer")?;

    let width = capturer.width() as u32;
    let height = capturer.height() as u32;

    // Try a few times (frame may not be ready)
    for _ in 0..10 {
        match capturer.frame() {
            Ok(frame) => {
                let rgb = bgra_to_rgb(&frame, width as usize, height as usize);
                let jpeg = encode_jpeg(&rgb, width, height, 85)?;
                return Ok(B64.encode(&jpeg));
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.into()),
        }
    }

    bail!("Failed to capture screenshot after retries")
}

/// Handle desktop-related CDAP messages.
pub(super) async fn handle_desktop_message(
    msg: &CdapMessage,
    sender: &Arc<Mutex<CdapSender>>,
    sessions: &mut SessionManager,
) -> Result<()> {
    match msg.msg_type.as_str() {
        "desktop_start" => {
            if let MessagePayload::DesktopStart {
                ref session_id,
                codec: _,
                ref quality,
                fps,
            } = msg.payload
            {
                let q = match quality.as_deref() {
                    Some("low") => 40,
                    Some("medium") => 60,
                    Some("high") => 80,
                    Some("best") => 95,
                    _ => 70,
                };

                let session = start_desktop_session(
                    session_id.clone(),
                    fps.max(1).min(30),
                    q,
                    sender.clone(),
                )?;
                sessions.desktop_session = Some(session);
            }
        }

        "desktop_input" => {
            if let MessagePayload::DesktopInput {
                ref input_type,
                ref params,
                ..
            } = msg.payload
            {
                handle_input(input_type, params)?;
            }
        }

        "desktop_stop" => {
            if let Some(session) = sessions.desktop_session.take() {
                session.stop().await;
            }
        }

        _ => {}
    }

    Ok(())
}

/// Process input event (mouse / keyboard).
fn handle_input(input_type: &str, params: &serde_json::Value) -> Result<()> {
    use enigo::{
        Coordinate, Direction, Enigo, Keyboard, Mouse, Settings,
    };

    let mut enigo = Enigo::new(&Settings::default()).context("Create Enigo")?;

    match input_type {
        "mouse_move" => {
            let x = params.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let y = params.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            enigo
                .move_mouse(x, y, Coordinate::Abs)
                .context("Mouse move")?;
        }

        "mouse_down" => {
            let button = parse_mouse_button(params);
            enigo
                .button(button, Direction::Press)
                .context("Mouse down")?;
        }

        "mouse_up" => {
            let button = parse_mouse_button(params);
            enigo
                .button(button, Direction::Release)
                .context("Mouse up")?;
        }

        "scroll" => {
            let dx = params.get("dx").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let dy = params.get("dy").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            if dy != 0 {
                enigo.scroll(dy, enigo::Axis::Vertical).context("Scroll V")?;
            }
            if dx != 0 {
                enigo
                    .scroll(dx, enigo::Axis::Horizontal)
                    .context("Scroll H")?;
            }
        }

        "key_down" => {
            let key = parse_key(params);
            enigo.key(key, Direction::Press).context("Key down")?;
        }

        "key_up" => {
            let key = parse_key(params);
            enigo.key(key, Direction::Release).context("Key up")?;
        }

        _ => {
            debug!("CDAP-desktop: Unknown input type: {}", input_type);
        }
    }

    Ok(())
}

fn parse_mouse_button(params: &serde_json::Value) -> enigo::Button {
    match params
        .get("button")
        .and_then(|v| v.as_str())
        .unwrap_or("left")
    {
        "right" => enigo::Button::Right,
        "middle" => enigo::Button::Middle,
        _ => enigo::Button::Left,
    }
}

fn parse_key(params: &serde_json::Value) -> enigo::Key {
    use enigo::Key;

    let key_str = params
        .get("key")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match key_str {
        "Enter" | "Return" => Key::Return,
        "Escape" | "Esc" => Key::Escape,
        "Tab" => Key::Tab,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "ArrowUp" | "Up" => Key::UpArrow,
        "ArrowDown" | "Down" => Key::DownArrow,
        "ArrowLeft" | "Left" => Key::LeftArrow,
        "ArrowRight" | "Right" => Key::RightArrow,
        "Space" | " " => Key::Space,
        "Control" | "Ctrl" => Key::Control,
        "Alt" => Key::Alt,
        "Shift" => Key::Shift,
        "Meta" | "Win" | "Super" => Key::Meta,
        "CapsLock" => Key::CapsLock,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        s if s.len() == 1 => Key::Unicode(s.chars().next().unwrap()),
        _ => Key::Unicode(' '),
    }
}

//! Remote desktop module — two modes of operation:
//!
//! ## 1. Management WS Agent (JPEG fallback — Phase 38)
//!
//! Connects to `ws://<host>:<port>/ws/remote-agent/<device_id>` and streams
//! JPEG frames when an operator starts a session from the web console.
//!
//! ## 2. Relay Session (H.264 — Phase 43)
//!
//! Uses the RustDesk-compatible relay connection established by `Session`.
//! Receives protobuf `VideoFrame` messages containing H.264 NALUs, decodes
//! via `openh264`, and emits RGBA frames to the frontend.
//!
//! Submodules:
//! - `video_pipeline`     — H.264 decoder + stats
//! - `input_pipeline`     — protobuf MouseEvent/KeyEvent builder
//! - `clipboard_sync`     — bidirectional clipboard
//! - `file_transfer_session` — in-session file transfer
//! - `session_manager`    — message dispatch loop
//! - `quality_monitor`    — FPS/latency/bandwidth tracking
//! - `session_recorder`   — recording to .bdrc file

pub mod clipboard_sync;
pub mod file_transfer_session;
pub mod input_pipeline;
pub mod quality_monitor;
pub mod session_manager;
pub mod session_recorder;
pub mod video_pipeline;

use image::{ImageEncoder, codecs::jpeg::JpegEncoder};
use log::{debug, error, info, warn};
use scrap::{Capturer, Display};
use serde::{Deserialize, Serialize};
use std::{
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use tauri::{AppHandle, Emitter, Runtime};
use tokio::{sync::mpsc, time::Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};

pub use session_manager::{SessionManager, SessionCommand, SessionQuality};

// ---------------------------------------------------------------------------
//  Constants
// ---------------------------------------------------------------------------

/// Target frames per second for the capture loop.
const TARGET_FPS: u64 = 15;
/// JPEG quality (0–100). 75 gives a good balance between quality and bandwidth.
const JPEG_QUALITY: u8 = 75;
/// Capture frame interval.
const FRAME_INTERVAL: Duration = Duration::from_millis(1000 / TARGET_FPS);

// ---------------------------------------------------------------------------
//  Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEvent {
    #[serde(rename = "type")]
    pub kind: String, // "mouse_move" | "mouse_down" | "mouse_up" | "wheel" | "key_down" | "key_up" | "key_press"
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub button: Option<String>, // "left" | "right" | "middle"
    pub delta_x: Option<i32>,
    pub delta_y: Option<i32>,
    pub key: Option<String>,    // key name e.g. "Return", "a", "Control"
    pub modifiers: Option<Vec<String>>, // ["ctrl", "shift", "alt", "meta"]
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteStatus {
    pub active: bool,
    pub standby: bool,
    pub frame_count: u64,
    pub fps: f32,
    pub width: u32,
    pub height: u32,
    pub error: Option<String>,
}

#[derive(Debug)]
enum AgentCommand {
    Stop,
}

// ---------------------------------------------------------------------------
//  Public service handle
// ---------------------------------------------------------------------------

pub struct RemoteAgent {
    sender: mpsc::Sender<AgentCommand>,
    active: Arc<AtomicBool>,
    standby: Arc<AtomicBool>,
    frame_count: Arc<Mutex<u64>>,
}

impl RemoteAgent {
    /// Start the agent in standby mode.  It will connect to the server and
    /// wait for a `start` signal.
    pub fn start<R: Runtime>(
        app: AppHandle<R>,
        ws_url: String,
        device_id: String,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<AgentCommand>(16);
        let active = Arc::new(AtomicBool::new(false));
        let standby = Arc::new(AtomicBool::new(true));
        let frame_count = Arc::new(Mutex::new(0u64));

        let active_cl = active.clone();
        let standby_cl = standby.clone();
        let fc_cl = frame_count.clone();

        tauri::async_runtime::spawn(async move {
            agent_loop(app, rx, ws_url, device_id, active_cl, standby_cl, fc_cl).await;
        });

        RemoteAgent { sender: tx, active, standby, frame_count }
    }

    /// Stop the agent.
    pub fn stop(&self) {
        let _ = self.sender.try_send(AgentCommand::Stop);
    }

    /// Current status snapshot.
    pub fn status(&self) -> RemoteStatus {
        RemoteStatus {
            active: self.active.load(Ordering::Relaxed),
            standby: self.standby.load(Ordering::Relaxed),
            frame_count: *self.frame_count.lock().unwrap(),
            fps: 0.0, // Computed by receiver; simplified here
            width: 0,
            height: 0,
            error: None,
        }
    }
}

// ---------------------------------------------------------------------------
//  Agent loop (background task)
// ---------------------------------------------------------------------------

async fn agent_loop<R: Runtime>(
    app: AppHandle<R>,
    mut rx: mpsc::Receiver<AgentCommand>,
    ws_url: String,
    device_id: String,
    active: Arc<AtomicBool>,
    standby: Arc<AtomicBool>,
    frame_count: Arc<Mutex<u64>>,
) {
    let mut retry_delay = Duration::from_secs(5);
    let max_delay = Duration::from_secs(60);

    loop {
        info!("RemoteAgent: connecting to {}", ws_url);

        match tokio::time::timeout(
            Duration::from_secs(15),
            connect_async(&ws_url),
        )
        .await
        {
            Ok(Ok((ws_stream, _))) => {
                retry_delay = Duration::from_secs(5);
                standby.store(true, Ordering::Relaxed);
                info!("RemoteAgent: standby — waiting for operator");
                emit_status_event(&app, &active, &standby, &frame_count, None);

                let (mut write, mut read) = ws_stream.split();

                // Announce ourselves
                let hello = serde_json::json!({
                    "type": "agent-ready",
                    "device_id": &device_id,
                });
                if let Ok(txt) = serde_json::to_string(&hello) {
                    let _ = write.send(Message::Text(txt.into())).await;
                }

                // Standby loop — wait for start/stop signals
                'standby: loop {
                    tokio::select! {
                        msg = read.next() => {
                            match msg {
                                Some(Ok(Message::Text(txt))) => {
                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                                        match val.get("type").and_then(|v| v.as_str()) {
                                            Some("start") => {
                                                info!("RemoteAgent: operator started session");
                                                active.store(true, Ordering::Relaxed);
                                                emit_status_event(&app, &active, &standby, &frame_count, None);

                                                // Run capture loop (blocking tokio task)
                                                let fc_cl = frame_count.clone();
                                                let active_cl = active.clone();
                                                let (frame_tx, mut frame_rx) = mpsc::channel::<Vec<u8>>(8);

                                                // Capture in a spawn_blocking thread (scrap is sync)
                                                let capture_handle = tokio::task::spawn_blocking(move || {
                                                    capture_loop_blocking(frame_tx, active_cl, fc_cl)
                                                });

                                                // Forward JPEG frames over WS, handle input events
                                                loop {
                                                    tokio::select! {
                                                        frame = frame_rx.recv() => {
                                                            match frame {
                                                                Some(jpeg) => {
                                                                    if write.send(Message::Binary(jpeg.into())).await.is_err() {
                                                                        break;
                                                                    }
                                                                }
                                                                None => break, // capturer stopped
                                                            }
                                                        }
                                                        msg = read.next() => {
                                                            match msg {
                                                                Some(Ok(Message::Text(txt))) => {
                                                                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&txt) {
                                                                        match val.get("type").and_then(|v| v.as_str()) {
                                                                            Some("stop") => {
                                                                                info!("RemoteAgent: operator stopped session");
                                                                                active.store(false, Ordering::Relaxed);
                                                                                break;
                                                                            }
                                                                            Some("input") => {
                                                                                if let Ok(input) = serde_json::from_value::<InputEvent>(val) {
                                                                                    inject_input(input);
                                                                                }
                                                                            }
                                                                            _ => {}
                                                                        }
                                                                    }
                                                                }
                                                                Some(Ok(Message::Close(_))) | None => {
                                                                    active.store(false, Ordering::Relaxed);
                                                                    break;
                                                                }
                                                                _ => {}
                                                            }
                                                        }
                                                        cmd = rx.recv() => {
                                                            if matches!(cmd, Some(AgentCommand::Stop) | None) {
                                                                active.store(false, Ordering::Relaxed);
                                                                standby.store(false, Ordering::Relaxed);
                                                                let _ = write.close().await;
                                                                return;
                                                            }
                                                        }
                                                    }
                                                }

                                                active.store(false, Ordering::Relaxed);
                                                emit_status_event(&app, &active, &standby, &frame_count, None);
                                                capture_handle.abort();
                                            }
                                            Some("ping") => {
                                                let pong = serde_json::json!({"type":"pong"});
                                                if let Ok(txt) = serde_json::to_string(&pong) {
                                                    let _ = write.send(Message::Text(txt.into())).await;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                Some(Ok(Message::Close(_))) | None => {
                                    warn!("RemoteAgent: server disconnected");
                                    break 'standby;
                                }
                                _ => {}
                            }
                        }
                        cmd = rx.recv() => {
                            if matches!(cmd, Some(AgentCommand::Stop) | None) {
                                standby.store(false, Ordering::Relaxed);
                                let _ = write.close().await;
                                return;
                            }
                        }
                    }
                }

                standby.store(false, Ordering::Relaxed);
                active.store(false, Ordering::Relaxed);
            }
            Ok(Err(e)) => {
                error!("RemoteAgent: connection failed: {}", e);
            }
            Err(_) => {
                error!("RemoteAgent: connection timed out (15s)");
            }
        }

        emit_status_event(&app, &active, &standby, &frame_count, None);

        // Wait before retrying, but respond to Stop
        tokio::select! {
            _ = tokio::time::sleep(retry_delay) => {}
            cmd = rx.recv() => {
                if matches!(cmd, Some(AgentCommand::Stop) | None) {
                    return;
                }
            }
        }

        retry_delay = (retry_delay * 2).min(max_delay);
    }
}

// ---------------------------------------------------------------------------
//  Screen capture (synchronous, runs in spawn_blocking)
// ---------------------------------------------------------------------------

fn capture_loop_blocking(
    tx: mpsc::Sender<Vec<u8>>,
    active: Arc<AtomicBool>,
    frame_count: Arc<Mutex<u64>>,
) {
    let display = match Display::primary() {
        Ok(d) => d,
        Err(e) => {
            error!("RemoteAgent capture: failed to get primary display: {}", e);
            return;
        }
    };

    let mut capturer = match Capturer::new(display) {
        Ok(c) => c,
        Err(e) => {
            error!("RemoteAgent capture: failed to create capturer: {}", e);
            return;
        }
    };

    let w = capturer.width();
    let h = capturer.height();
    info!("RemoteAgent capture: {}x{} @ {}fps", w, h, TARGET_FPS);

    while active.load(Ordering::Relaxed) {
        let frame_start = std::time::Instant::now();

        match capturer.frame() {
            Ok(frame) => {
                // scrap returns BGRA on Windows with possible row padding
                // (DXGI stride may exceed width*4). Extract only the valid
                // pixels per row.
                let stride = frame.len() / h;
                let mut rgb = Vec::with_capacity(w * h * 3);
                for row in 0..h {
                    let row_start = row * stride;
                    for col in 0..w {
                        let px = row_start + col * 4;
                        rgb.push(frame[px + 2]); // R
                        rgb.push(frame[px + 1]); // G
                        rgb.push(frame[px]);     // B
                    }
                }

                // Encode to JPEG
                let jpeg = encode_jpeg(&rgb, w as u32, h as u32, JPEG_QUALITY);

                *frame_count.lock().unwrap() += 1;

                // Non-blocking send — drop frame if channel is full
                if tx.blocking_send(jpeg).is_err() {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Frame not ready, sleep briefly
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
            Err(e) => {
                error!("RemoteAgent capture: frame error: {}", e);
                break;
            }
        }

        // Throttle to TARGET_FPS
        let elapsed = frame_start.elapsed();
        if elapsed < FRAME_INTERVAL {
            std::thread::sleep(FRAME_INTERVAL - elapsed);
        }
    }

    info!("RemoteAgent capture: loop ended");
}

fn encode_jpeg(rgb: &[u8], width: u32, height: u32, quality: u8) -> Vec<u8> {
    let mut buf = Cursor::new(Vec::with_capacity(rgb.len() / 4));
    let encoder = JpegEncoder::new_with_quality(&mut buf, quality);
    if let Err(e) = encoder.write_image(rgb, width, height, image::ExtendedColorType::Rgb8) {
        warn!("JPEG encode error: {}", e);
        return Vec::new();
    }
    buf.into_inner()
}

// ---------------------------------------------------------------------------
//  Input injection
// ---------------------------------------------------------------------------

fn inject_input(event: InputEvent) {
    use enigo::{
        Coordinate, Direction, Enigo, Keyboard, Mouse, Settings,
    };

    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        warn!("inject_input: failed to create Enigo");
        return;
    };

    match event.kind.as_str() {
        "mouse_move" => {
            if let (Some(x), Some(y)) = (event.x, event.y) {
                let _ = enigo.move_mouse(x, y, Coordinate::Abs);
            }
        }
        "mouse_down" => {
            let btn = parse_mouse_button(event.button.as_deref());
            let _ = enigo.button(btn, Direction::Press);
        }
        "mouse_up" => {
            let btn = parse_mouse_button(event.button.as_deref());
            let _ = enigo.button(btn, Direction::Release);
        }
        "wheel" => {
            if let Some(dy) = event.delta_y {
                let _ = enigo.scroll(dy, enigo::Axis::Vertical);
            }
        }
        "key_down" => {
            if let Some(k) = event.key.as_deref() {
                if let Some(key) = parse_key(k) {
                    let _ = enigo.key(key, Direction::Press);
                }
            }
        }
        "key_up" => {
            if let Some(k) = event.key.as_deref() {
                if let Some(key) = parse_key(k) {
                    let _ = enigo.key(key, Direction::Release);
                }
            }
        }
        "key_press" => {
            if let Some(k) = event.key.as_deref() {
                if let Some(key) = parse_key(k) {
                    let _ = enigo.key(key, Direction::Click);
                }
            }
        }
        other => {
            debug!("inject_input: unknown event type: {}", other);
        }
    }
}

fn parse_mouse_button(btn: Option<&str>) -> enigo::Button {
    match btn {
        Some("right") => enigo::Button::Right,
        Some("middle") => enigo::Button::Middle,
        _ => enigo::Button::Left,
    }
}

fn parse_key(key: &str) -> Option<enigo::Key> {
    use enigo::Key;
    Some(match key {
        "Return" | "Enter" => Key::Return,
        "Escape" => Key::Escape,
        "Tab" => Key::Tab,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Insert" => Key::Insert,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "ArrowLeft" | "Left" => Key::LeftArrow,
        "ArrowRight" | "Right" => Key::RightArrow,
        "ArrowUp" | "Up" => Key::UpArrow,
        "ArrowDown" | "Down" => Key::DownArrow,
        "F1" => Key::F1, "F2" => Key::F2, "F3" => Key::F3, "F4" => Key::F4,
        "F5" => Key::F5, "F6" => Key::F6, "F7" => Key::F7, "F8" => Key::F8,
        "F9" => Key::F9, "F10" => Key::F10, "F11" => Key::F11, "F12" => Key::F12,
        "Control" | "ControlLeft" | "ControlRight" => Key::Control,
        "Alt" | "AltLeft" | "AltRight" => Key::Alt,
        "Shift" | "ShiftLeft" | "ShiftRight" => Key::Shift,
        "Meta" | "MetaLeft" | "MetaRight" | "Super" => Key::Meta,
        "Space" | " " => Key::Space,
        k if k.len() == 1 => {
            Key::Unicode(k.chars().next().unwrap())
        }
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

fn emit_status_event<R: Runtime>(
    app: &AppHandle<R>,
    active: &Arc<AtomicBool>,
    standby: &Arc<AtomicBool>,
    frame_count: &Arc<Mutex<u64>>,
    error: Option<String>,
) {
    let status = RemoteStatus {
        active: active.load(Ordering::Relaxed),
        standby: standby.load(Ordering::Relaxed),
        frame_count: *frame_count.lock().unwrap(),
        fps: 0.0,
        width: 0,
        height: 0,
        error,
    };
    let _ = app.emit("remote-status", &status);
}

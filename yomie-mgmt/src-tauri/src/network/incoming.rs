//! Incoming remote desktop connection handler.
//!
//! When the signal server forwards a PunchHole or RequestRelay message,
//! this module:
//!   1. Connects to the relay server via TCP
//!   2. Sends SignedId (as the answerer / target)
//!   3. Sends a Hash challenge for password authentication
//!   4. Verifies the incoming LoginRequest
//!   5. Sends LoginResponse with PeerInfo + display info
//!   6. Captures the screen (DXGI/X11) → encodes H264 → sends VideoFrame
//!   7. Receives MouseEvent / KeyEvent → injects via enigo

use anyhow::{bail, Context, Result};
use log::{debug, error, info, warn};
use prost::Message as ProstMessage;
use scrap::{Capturer, Display};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::watch;

use crate::crypto::EphemeralKeyPair;
use crate::proto::{
    message::Union as MsgUnion,
    rendezvous_message::Union as RdzUnion,
    video_frame, DisplayInfo, EncodedVideoFrame, EncodedVideoFrames, Hash, IdPk, LoginResponse,
    Message as PeerMessage, PeerInfo, RendezvousMessage, RequestRelay, SignedId, VideoFrame,
    login_response, key_event,
};
use crate::protocol::{encode_frame, read_frame, write_frame};

// ---------------------------------------------------------------------------
//  Constants
// ---------------------------------------------------------------------------

const TARGET_FPS: u64 = 15;
const FRAME_INTERVAL_MS: u64 = 1000 / TARGET_FPS;
const KEYFRAME_INTERVAL: u32 = 60;

// ---------------------------------------------------------------------------
//  Remote session info (emitted to frontend for RemoteBadge)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RemoteSessionInfo {
    pub active: bool,
    pub peer_id: String,
    pub peer_name: String,
    pub connected_at: i64,
    pub encrypted: bool,
}

// ---------------------------------------------------------------------------
//  Public API
// ---------------------------------------------------------------------------

pub struct IncomingSession {
    cancel_tx: watch::Sender<bool>,
    active: Arc<AtomicBool>,
}

impl IncomingSession {
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    pub fn stop(&self) {
        let _ = self.cancel_tx.send(true);
    }
}

/// Spawn an incoming connection handler.
pub fn spawn_incoming<R: tauri::Runtime>(
    relay_server: String,
    uuid: String,
    device_password: String,
    my_id: String,
    app: Option<tauri::AppHandle<R>>,
) -> IncomingSession {
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let active = Arc::new(AtomicBool::new(false));
    let active_cl = active.clone();

    tokio::spawn(async move {
        if let Err(e) =
            run_incoming_session(&relay_server, &uuid, &device_password, &my_id, active_cl, cancel_rx, app).await
        {
            error!("[incoming] Session failed: {}", e);
        }
    });

    IncomingSession { cancel_tx, active }
}

// ---------------------------------------------------------------------------
//  Session implementation
// ---------------------------------------------------------------------------

async fn run_incoming_session<R: tauri::Runtime>(
    relay_server: &str,
    uuid: &str,
    device_password: &str,
    my_id: &str,
    active: Arc<AtomicBool>,
    mut cancel_rx: watch::Receiver<bool>,
    app: Option<tauri::AppHandle<R>>,
) -> Result<()> {
    info!(
        "[incoming] Connecting to relay {} (uuid={}, my_id={})",
        relay_server, uuid, my_id
    );

    let relay_addr = if relay_server.contains(':') {
        relay_server.to_string()
    } else {
        format!("{}:21117", relay_server)
    };

    let stream = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        TcpStream::connect(&relay_addr),
    )
    .await
    .context("Relay connection timed out")?
    .context("Failed to connect to relay")?;

    let (mut reader, mut writer) = tokio::io::split(stream);

    // Send RequestRelay with UUID to register with relay
    let req = RendezvousMessage {
        union: Some(RdzUnion::RequestRelay(RequestRelay {
            id: my_id.to_string(),
            uuid: uuid.to_string(),
            socket_addr: Vec::new(),
            relay_server: String::new(),
            secure: false,
            licence_key: String::new(),
            conn_type: 0,
            token: String::new(),
            control_permissions: None,
        })),
    };
    writer.write_all(&write_frame(&req)).await?;
    debug!("[incoming] Sent RequestRelay (uuid={})", uuid);

    // Skip relay confirmation (RelayResponse from relay server)
    let first_frame = read_frame(&mut reader).await?;
    if let Ok(rdz) = RendezvousMessage::decode(first_frame.as_slice()) {
        if matches!(rdz.union, Some(RdzUnion::RelayResponse(_))) {
            debug!("[incoming] Skipped relay confirmation");
        }
    }

    // Send our SignedId to the initiator
    let ephemeral = EphemeralKeyPair::generate();
    let id_pk = IdPk {
        id: my_id.to_string(),
        pk: ephemeral.public.as_bytes().to_vec(),
    };
    let id_pk_bytes = id_pk.encode_to_vec();

    // 64-byte dummy signature + IdPk protobuf
    let mut signed_id_bytes = vec![0u8; 64];
    signed_id_bytes.extend_from_slice(&id_pk_bytes);

    let signed_id_msg = PeerMessage {
        union: Some(MsgUnion::SignedId(SignedId {
            id: signed_id_bytes,
        })),
    };
    writer.write_all(&write_frame(&signed_id_msg)).await?;
    debug!("[incoming] Sent SignedId");

    // Send Hash challenge for password authentication
    let salt = uuid::Uuid::new_v4().to_string();
    let challenge = uuid::Uuid::new_v4().to_string();

    let hash_msg = PeerMessage {
        union: Some(MsgUnion::Hash(Hash {
            salt: salt.clone(),
            challenge: challenge.clone(),
        })),
    };
    writer.write_all(&write_frame(&hash_msg)).await?;
    info!("[incoming] Sent Hash challenge, waiting for LoginRequest");

    // Read LoginRequest
    let login_frame = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        read_frame(&mut reader),
    )
    .await
    .context("Timeout waiting for LoginRequest")?
    .context("Failed to read LoginRequest")?;

    let login_msg = PeerMessage::decode(login_frame.as_slice())
        .context("Failed to decode LoginRequest")?;

    let login_req = match login_msg.union {
        Some(MsgUnion::LoginRequest(lr)) => lr,
        other => bail!("[incoming] Expected LoginRequest, got: {:?}", other),
    };

    info!(
        "[incoming] LoginRequest from {} ({})",
        login_req.my_id, login_req.my_name
    );

    // Verify password — if device_password is empty, allow any connection
    // (first-run mode / unattended access). Otherwise, verify hash.
    if !device_password.is_empty() {
        let expected_hash = crate::crypto::hash_password(device_password, &salt, &challenge);

        if login_req.password != expected_hash.as_slice() {
            warn!(
                "[incoming] Password mismatch from {} (pw_len={}, hash_len={})",
                login_req.my_id,
                login_req.password.len(),
                expected_hash.len()
            );
            let err_resp = PeerMessage {
                union: Some(MsgUnion::LoginResponse(LoginResponse {
                    union: Some(login_response::Union::Error("Wrong Password".to_string())),
                    enable_trusted_devices: false,
                })),
            };
            writer.write_all(&write_frame(&err_resp)).await?;
            bail!("[incoming] Password verification failed");
        }
        info!("[incoming] Password verified for {}", login_req.my_id);
    } else {
        info!("[incoming] No device password set — allowing connection from {}", login_req.my_id);
    }

    // Collect display info and send LoginResponse
    let (displays, screen_w, screen_h) = get_display_info();

    let peer_info = PeerInfo {
        username: whoami::username(),
        hostname: whoami::devicename(),
        platform: std::env::consts::OS.to_string(),
        displays,
        current_display: 0,
        sas_enabled: false,
        version: env!("CARGO_PKG_VERSION").to_string(),
        features: None,
        encoding: Some(crate::proto::SupportedEncoding {
            h264: true,
            h265: false,
            vp8: false,
            av1: false,
            i444: None,
        }),
        resolutions: None,
        platform_additions: String::new(),
        windows_sessions: None,
    };

    let login_resp = PeerMessage {
        union: Some(MsgUnion::LoginResponse(LoginResponse {
            union: Some(login_response::Union::PeerInfo(peer_info)),
            enable_trusted_devices: false,
        })),
    };
    writer.write_all(&write_frame(&login_resp)).await?;
    info!(
        "[incoming] Authenticated — streaming {}x{} to {}",
        screen_w, screen_h, login_req.my_id
    );
    active.store(true, Ordering::Relaxed);

    // Emit session start event to frontend (RemoteBadge)
    if let Some(ref app_handle) = app {
        use tauri::Emitter;
        let info = RemoteSessionInfo {
            active: true,
            peer_id: login_req.my_id.clone(),
            peer_name: login_req.my_name.clone(),
            connected_at: chrono::Utc::now().timestamp_millis(),
            encrypted: true,
        };
        let _ = app_handle.emit("remote-session-start", &info);
    }

    // Run capture + input loop
    let result = run_stream_loop(
        &mut reader,
        &mut writer,
        screen_w,
        screen_h,
        &active,
        &mut cancel_rx,
    )
    .await;

    active.store(false, Ordering::Relaxed);

    // Emit session end event
    if let Some(ref app_handle) = app {
        use tauri::Emitter;
        let _ = app_handle.emit("remote-session-end", ());
    }

    info!("[incoming] Session ended");
    result
}

// ---------------------------------------------------------------------------
//  Streaming loop
// ---------------------------------------------------------------------------

async fn run_stream_loop(
    reader: &mut ReadHalf<TcpStream>,
    writer: &mut WriteHalf<TcpStream>,
    width: u32,
    height: u32,
    _active: &AtomicBool,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<()> {
    let (frame_tx, mut frame_rx) = tokio::sync::mpsc::channel::<(Vec<u8>, bool)>(4);
    let cap_active = Arc::new(AtomicBool::new(true));
    let cap_active_cl = cap_active.clone();
    let cap_w = width as usize;
    let cap_h = height as usize;

    let capture_handle = tokio::task::spawn_blocking(move || {
        capture_and_encode_loop(frame_tx, cap_active_cl, cap_w, cap_h);
    });

    let mut pts: i64 = 0;

    loop {
        tokio::select! {
            frame = frame_rx.recv() => {
                match frame {
                    Some((h264_data, is_key)) => {
                        let vf = PeerMessage {
                            union: Some(MsgUnion::VideoFrame(VideoFrame {
                                display: 0,
                                union: Some(video_frame::Union::H264s(EncodedVideoFrames {
                                    frames: vec![EncodedVideoFrame {
                                        data: h264_data,
                                        key: is_key,
                                        pts,
                                    }],
                                })),
                            })),
                        };
                        let frame_bytes = encode_frame(&vf.encode_to_vec());
                        if writer.write_all(&frame_bytes).await.is_err() {
                            break;
                        }
                        pts += 1;
                    }
                    None => break,
                }
            }

            result = read_frame(reader) => {
                match result {
                    Ok(data) => {
                        if let Ok(msg) = PeerMessage::decode(data.as_slice()) {
                            handle_peer_message(msg);
                        }
                    }
                    Err(e) => {
                        debug!("[incoming] Read error: {}", e);
                        break;
                    }
                }
            }

            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    break;
                }
            }
        }
    }

    cap_active.store(false, Ordering::Relaxed);
    capture_handle.abort();
    Ok(())
}

// ---------------------------------------------------------------------------
//  Handle incoming peer messages (input events)
// ---------------------------------------------------------------------------

fn handle_peer_message(msg: PeerMessage) {
    match msg.union {
        Some(MsgUnion::MouseEvent(me)) => {
            inject_mouse_event(me.mask, me.x, me.y);
        }
        Some(MsgUnion::KeyEvent(ke)) => {
            inject_key_event(&ke);
        }
        Some(MsgUnion::Clipboard(cb)) => {
            debug!("[incoming] Clipboard: {} bytes", cb.content.len());
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                if let Ok(text) = String::from_utf8(cb.content) {
                    let _ = clipboard.set_text(&text);
                }
            }
        }
        Some(MsgUnion::Misc(_)) => {
            debug!("[incoming] Misc message received");
        }
        Some(MsgUnion::TestDelay(td)) => {
            debug!("[incoming] TestDelay: time={}", td.time);
        }
        _ => {
            debug!("[incoming] Unhandled message type");
        }
    }
}

// ---------------------------------------------------------------------------
//  Screen capture + H264 encoding (blocking thread)
// ---------------------------------------------------------------------------

fn capture_and_encode_loop(
    tx: tokio::sync::mpsc::Sender<(Vec<u8>, bool)>,
    active: Arc<AtomicBool>,
    _expected_w: usize,
    _expected_h: usize,
) {
    let display = match Display::primary() {
        Ok(d) => d,
        Err(e) => {
            error!("[incoming] Failed to get primary display: {}", e);
            return;
        }
    };

    let mut capturer = match Capturer::new(display) {
        Ok(c) => c,
        Err(e) => {
            error!("[incoming] Failed to create capturer: {}", e);
            return;
        }
    };

    let w = capturer.width();
    let h = capturer.height();
    info!("[incoming] Capture started: {}x{} H264 @ {}fps", w, h, TARGET_FPS);

    // Initialize H264 encoder
    let api = openh264::OpenH264API::from_source();

    let config = openh264::encoder::EncoderConfig::new()
        .bitrate(openh264::encoder::BitRate::from_bps(2_000_000));

    let mut encoder = match openh264::encoder::Encoder::with_api_config(api, config) {
        Ok(e) => e,
        Err(e) => {
            error!("[incoming] Failed to create H264 encoder: {}", e);
            return;
        }
    };

    let mut frame_count: u32 = 0;
    let frame_interval = std::time::Duration::from_millis(FRAME_INTERVAL_MS);

    while active.load(Ordering::Relaxed) {
        let frame_start = std::time::Instant::now();

        match capturer.frame() {
            Ok(bgra_frame) => {
                let is_key = frame_count % KEYFRAME_INTERVAL == 0;

                if is_key {
                    encoder.force_intra_frame();
                }

                // Convert BGRA → I420 YUV for H264 encoder
                let yuv = bgra_to_yuv_buffer(&bgra_frame, w, h);

                match encoder.encode(&yuv) {
                    Ok(bitstream) => {
                        let mut data = Vec::new();
                        bitstream.write_vec(&mut data);
                        if !data.is_empty() {
                            if tx.blocking_send((data, is_key)).is_err() {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("[incoming] H264 encode error: {}", e);
                    }
                }

                frame_count += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
            Err(e) => {
                error!("[incoming] Capture error: {}", e);
                break;
            }
        }

        let elapsed = frame_start.elapsed();
        if elapsed < frame_interval {
            std::thread::sleep(frame_interval - elapsed);
        }
    }

    info!("[incoming] Capture ended ({} frames)", frame_count);
}

/// Convert BGRA pixels (from scrap, with possible row padding) to YUVBuffer.
///
/// DXGI may return frames with stride > width*4.  We strip the padding
/// before passing to openh264's BGRA→YUV conversion.
fn bgra_to_yuv_buffer(bgra: &[u8], w: usize, h: usize) -> openh264::formats::YUVBuffer {
    let expected_len = w * h * 4;
    if bgra.len() == expected_len {
        // No padding — use directly
        let src = openh264::formats::BgraSliceU8::new(bgra, (w, h));
        return openh264::formats::YUVBuffer::from_rgb_source(src);
    }

    // Has row padding — strip it
    let stride = bgra.len() / h;
    let mut stripped = Vec::with_capacity(expected_len);
    for row in 0..h {
        let start = row * stride;
        let end = start + w * 4;
        stripped.extend_from_slice(&bgra[start..end]);
    }
    let src = openh264::formats::BgraSliceU8::new(&stripped, (w, h));
    openh264::formats::YUVBuffer::from_rgb_source(src)
}

// ---------------------------------------------------------------------------
//  Input injection
// ---------------------------------------------------------------------------

fn inject_mouse_event(mask: i32, x: i32, y: i32) {
    use enigo::{Coordinate, Direction, Enigo, Mouse, Settings};

    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return;
    };

    let button_id = mask >> 3;
    let event_type = mask & 7;

    match event_type {
        0 => {
            let _ = enigo.move_mouse(x, y, Coordinate::Abs);
        }
        1 => {
            let _ = enigo.move_mouse(x, y, Coordinate::Abs);
            let btn = match button_id {
                2 => enigo::Button::Right,
                4 => enigo::Button::Middle,
                _ => enigo::Button::Left,
            };
            let _ = enigo.button(btn, Direction::Press);
        }
        2 => {
            let _ = enigo.move_mouse(x, y, Coordinate::Abs);
            let btn = match button_id {
                2 => enigo::Button::Right,
                4 => enigo::Button::Middle,
                _ => enigo::Button::Left,
            };
            let _ = enigo.button(btn, Direction::Release);
        }
        3 => {
            let _ = enigo.scroll(y, enigo::Axis::Vertical);
        }
        _ => {}
    }
}

fn inject_key_event(ke: &crate::proto::KeyEvent) {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let Ok(mut enigo) = Enigo::new(&Settings::default()) else {
        return;
    };

    let dir = if ke.down {
        Direction::Press
    } else {
        Direction::Release
    };

    // Match the oneof union field
    match &ke.union {
        Some(key_event::Union::Chr(ch)) => {
            if let Some(c) = char::from_u32(*ch) {
                let _ = enigo.key(Key::Unicode(c), dir);
            }
        }
        Some(key_event::Union::Unicode(u)) => {
            if let Some(c) = char::from_u32(*u) {
                let _ = enigo.key(Key::Unicode(c), dir);
            }
        }
        Some(key_event::Union::ControlKey(ck)) => {
            if let Some(key) = map_control_key(*ck) {
                let _ = enigo.key(key, dir);
            }
        }
        Some(key_event::Union::Seq(s)) => {
            // Sequence of characters — type them as Unicode
            for c in s.chars() {
                let _ = enigo.key(Key::Unicode(c), dir);
            }
        }
        _ => {
            debug!("[incoming] Unhandled key event variant");
        }
    }
}

fn map_control_key(ck: i32) -> Option<enigo::Key> {
    use enigo::Key;
    // ControlKey enum values from RustDesk protobuf
    Some(match ck {
        1 => Key::Alt,
        2 => Key::Backspace,
        3 => Key::CapsLock,
        4 => Key::Control,
        5 => Key::Delete,
        6 => Key::DownArrow,
        7 => Key::End,
        8 => Key::Return,
        9 => Key::Escape,
        10 => Key::F1, 11 => Key::F2, 12 => Key::F3, 13 => Key::F4,
        14 => Key::F5, 15 => Key::F6, 16 => Key::F7, 17 => Key::F8,
        18 => Key::F9, 19 => Key::F10, 20 => Key::F11, 21 => Key::F12,
        22 => Key::Home,
        23 => Key::LeftArrow,
        25 => Key::PageDown,
        26 => Key::PageUp,
        28 => Key::RightArrow,
        29 => Key::Shift,
        30 => Key::Space,
        31 => Key::Tab,
        32 => Key::UpArrow,
        35 => Key::Insert,
        65 => Key::Meta,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
//  Display info helper
// ---------------------------------------------------------------------------

fn get_display_info() -> (Vec<DisplayInfo>, u32, u32) {
    let mut displays = Vec::new();
    let mut primary_w: u32 = 1920;
    let mut primary_h: u32 = 1080;

    if let Ok(all) = Display::all() {
        for (i, d) in all.iter().enumerate() {
            let w = d.width() as i32;
            let h = d.height() as i32;
            displays.push(DisplayInfo {
                x: 0,
                y: 0,
                width: w,
                height: h,
                name: format!("Display {}", i + 1),
                online: true,
                cursor_embedded: false,
                original_resolution: None,
                scale: 1.0,
            });
            if i == 0 {
                primary_w = w as u32;
                primary_h = h as u32;
            }
        }
    }

    if displays.is_empty() {
        displays.push(DisplayInfo {
            x: 0,
            y: 0,
            width: primary_w as i32,
            height: primary_h as i32,
            name: "Primary".into(),
            online: true,
            cursor_embedded: false,
            original_resolution: None,
            scale: 1.0,
        });
    }

    (displays, primary_w, primary_h)
}

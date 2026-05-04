//! Session manager — orchestrates the relay message loop for a remote desktop
//! session.  Dispatches incoming protobuf messages to the appropriate pipeline
//! (video, clipboard, file transfer, cursor) and forwards outgoing input
//! events to the peer.

use anyhow::{Context, Result};
use log::{debug, error, info};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::mpsc;

use crate::proto::{
    message::Union as MsgUnion, misc, Message as PeerMessage, Misc, OptionMessage,
    SwitchDisplay, TestDelay,
};

use super::clipboard_sync::ClipboardSync;
use super::file_transfer_session::FileTransferSession;
use super::input_pipeline;
use super::quality_monitor::QualityMonitor;
use super::video_pipeline::VideoPipeline;

// ---------------------------------------------------------------------------
//  Types
// ---------------------------------------------------------------------------

/// Commands sent from the frontend to the session manager.
#[derive(Debug)]
pub enum SessionCommand {
    /// Mouse move
    MouseMove { x: i32, y: i32 },
    /// Mouse button press/release
    MouseButton { x: i32, y: i32, button: u8, down: bool, modifiers: Vec<String> },
    /// Mouse wheel
    MouseWheel { x: i32, y: i32, delta_x: i32, delta_y: i32 },
    /// Key press/release
    Key { key: String, down: bool, modifiers: Vec<String> },
    /// Special key (Ctrl+Alt+Del, LockScreen)
    SpecialKey { key: String },
    /// Toggle clipboard sync
    ToggleClipboard { enabled: bool },
    /// Switch display
    SwitchDisplay { index: i32 },
    /// Set quality options
    SetQuality { image_quality: String, fps: u32 },
    /// Request refresh (keyframe)
    RefreshVideo,
    /// Browse remote directory
    BrowseRemote { path: String },
    /// Download remote file
    DownloadFile { remote_path: String, filename: String },
    /// Cancel transfer
    CancelTransfer { id: i32 },
    /// Toggle recording
    ToggleRecording { enabled: bool },
    /// Stop session
    Stop,
}

/// Cursor update event for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct CursorEvent {
    pub id: u64,
    pub hotx: i32,
    pub hoty: i32,
    pub width: i32,
    pub height: i32,
    #[serde(skip_serializing)]
    pub colors: Vec<u8>,
    /// Base64-encoded RGBA pixel data
    pub colors_b64: String,
}

/// Session quality stats for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct SessionQuality {
    pub fps: f32,
    pub latency_ms: u32,
    pub bandwidth_kbps: f64,
    pub frames_decoded: u64,
    pub frames_dropped: u64,
    pub codec: String,
    pub width: u32,
    pub height: u32,
}

// ---------------------------------------------------------------------------
//  Session Manager
// ---------------------------------------------------------------------------

/// Manages a live remote desktop session.
pub struct SessionManager {
    cmd_tx: mpsc::Sender<SessionCommand>,
}

impl SessionManager {
    /// Start a new session manager.
    ///
    /// Takes ownership of the relay connection (via the session's send/receive
    /// channels) and spawns the message dispatch loop.
    pub fn start<R: Runtime>(
        app: AppHandle<R>,
        msg_rx: mpsc::Receiver<PeerMessage>,
        msg_tx: mpsc::Sender<PeerMessage>,
    ) -> Result<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<SessionCommand>(256);

        let video = VideoPipeline::new().context("Failed to create video pipeline")?;

        tokio::spawn(async move {
            if let Err(e) = session_loop(app, msg_rx, msg_tx, cmd_rx, video).await {
                error!("Session loop error: {}", e);
            }
        });

        Ok(Self { cmd_tx })
    }

    /// Send a command to the session.
    pub fn send(&self, cmd: SessionCommand) {
        if self.cmd_tx.try_send(cmd).is_err() {
            debug!("SessionManager: command channel full or closed");
        }
    }

    /// Stop the session.
    pub fn stop(&self) {
        let _ = self.cmd_tx.try_send(SessionCommand::Stop);
    }
}

// ---------------------------------------------------------------------------
//  Main session loop
// ---------------------------------------------------------------------------

async fn session_loop<R: Runtime>(
    app: AppHandle<R>,
    mut msg_rx: mpsc::Receiver<PeerMessage>,
    msg_tx: mpsc::Sender<PeerMessage>,
    mut cmd_rx: mpsc::Receiver<SessionCommand>,
    mut video: VideoPipeline,
) -> Result<()> {
    info!("SessionManager: session loop started");

    let mut clipboard = ClipboardSync::new();
    let mut file_transfer = FileTransferSession::new();
    let mut quality = QualityMonitor::new();

    let mut clipboard_poll = tokio::time::interval(Duration::from_millis(500));
    let mut quality_poll = tokio::time::interval(Duration::from_secs(2));

    // Send initial video_received ack
    send_video_received(&msg_tx).await;

    loop {
        tokio::select! {
            // Incoming protobuf messages from the peer
            msg = msg_rx.recv() => {
                match msg {
                    Some(peer_msg) => {
                        handle_peer_message(
                            &app, &peer_msg, &mut video, &mut clipboard,
                            &mut file_transfer, &mut quality, &msg_tx,
                        ).await;
                    }
                    None => {
                        info!("SessionManager: relay channel closed — connection lost");
                        let _ = app.emit("remote-viewer-status", serde_json::json!({
                            "connected": false,
                            "error": "Connection lost — relay channel closed"
                        }));
                        break;
                    }
                }
            }

            // Commands from the frontend
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(SessionCommand::Stop) | None => {
                        info!("SessionManager: stop requested");
                        let _ = app.emit("remote-viewer-status", serde_json::json!({
                            "connected": false
                        }));
                        break;
                    }
                    Some(cmd) => {
                        handle_command(&cmd, &msg_tx, &mut clipboard, &mut file_transfer, &app).await;
                    }
                }
            }

            // Periodic clipboard poll
            _ = clipboard_poll.tick() => {
                if let Some(msg) = clipboard.poll_local() {
                    let _ = msg_tx.send(msg).await;
                }
                // Check for file transfer dir results
                let dirs = file_transfer.take_dir_results();
                for dir in dirs {
                    let _ = app.emit("remote-dir-result", &dir);
                }
            }

            // Periodic quality stats
            _ = quality_poll.tick() => {
                let vstats = video.stats();
                let q = SessionQuality {
                    fps: vstats.fps,
                    latency_ms: quality.latency_ms(),
                    bandwidth_kbps: quality.bandwidth_kbps(),
                    frames_decoded: vstats.frames_decoded,
                    frames_dropped: vstats.frames_dropped,
                    codec: vstats.codec,
                    width: vstats.width,
                    height: vstats.height,
                };
                let _ = app.emit("remote-quality", &q);
            }
        }
    }

    info!("SessionManager: session loop ended");
    Ok(())
}

// ---------------------------------------------------------------------------
//  Message handlers
// ---------------------------------------------------------------------------

async fn handle_peer_message<R: Runtime>(
    app: &AppHandle<R>,
    msg: &PeerMessage,
    video: &mut VideoPipeline,
    clipboard: &mut ClipboardSync,
    file_transfer: &mut FileTransferSession,
    quality: &mut QualityMonitor,
    msg_tx: &mpsc::Sender<PeerMessage>,
) {
    match &msg.union {
        Some(MsgUnion::VideoFrame(vf)) => {
            let frames = video.process_video_frame(vf);
            for frame in frames {
                // Emit RGBA frame as base64 to frontend
                let b64 = base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &frame.rgba,
                );
                let _ = app.emit("remote-frame", serde_json::json!({
                    "width": frame.width,
                    "height": frame.height,
                    "rgba_b64": b64,
                }));
            }
            // Send video_received ack for flow control
            send_video_received(msg_tx).await;
            quality.record_frame(msg.encode_to_vec().len());
        }

        Some(MsgUnion::CursorData(cd)) => {
            let colors_b64 = base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &cd.colors,
            );
            let event = CursorEvent {
                id: cd.id,
                hotx: cd.hotx,
                hoty: cd.hoty,
                width: cd.width,
                height: cd.height,
                colors: cd.colors.clone(),
                colors_b64,
            };
            let _ = app.emit("remote-cursor", &event);
        }

        Some(MsgUnion::CursorPosition(cp)) => {
            let _ = app.emit("remote-cursor-pos", serde_json::json!({
                "x": cp.x,
                "y": cp.y,
            }));
        }

        Some(MsgUnion::CursorId(id)) => {
            let _ = app.emit("remote-cursor-id", id);
        }

        Some(MsgUnion::Clipboard(cb)) => {
            if let Err(e) = clipboard.handle_remote(cb) {
                debug!("Clipboard sync error: {}", e);
            }
        }

        Some(MsgUnion::FileResponse(fr)) => {
            if let Some(progress) = file_transfer.handle_response(fr) {
                let _ = app.emit("remote-transfer-progress", &progress);
            }
        }

        Some(MsgUnion::TestDelay(td)) => {
            quality.handle_test_delay(td);
            // Echo back for RTT measurement
            let reply = PeerMessage {
                union: Some(MsgUnion::TestDelay(TestDelay {
                    time: td.time,
                    from_client: true,
                    last_delay: td.last_delay,
                    target_bitrate: td.target_bitrate,
                })),
            };
            let _ = msg_tx.send(reply).await;
        }

        Some(MsgUnion::Misc(misc_msg)) => {
            handle_misc(app, misc_msg);
        }

        Some(MsgUnion::LoginResponse(_)) => {
            // Already handled by Session — should not reach here
            debug!("SessionManager: ignoring late LoginResponse");
        }

        Some(MsgUnion::AudioFrame(_af)) => {
            // Audio playback not implemented yet
        }

        other => {
            debug!("SessionManager: unhandled message type: {:?}", other.as_ref().map(|_| "..."));
        }
    }
}

fn handle_misc<R: Runtime>(app: &AppHandle<R>, misc_msg: &Misc) {
    match &misc_msg.union {
        Some(misc::Union::SwitchDisplay(sd)) => {
            let _ = app.emit("remote-display-switch", serde_json::json!({
                "display": sd.display,
                "x": sd.x,
                "y": sd.y,
                "width": sd.width,
                "height": sd.height,
            }));
        }
        Some(misc::Union::ChatMessage(cm)) => {
            let _ = app.emit("remote-chat", &cm.text);
        }
        Some(misc::Union::CloseReason(reason)) => {
            info!("SessionManager: peer closed: {}", reason);
            let _ = app.emit("remote-closed", reason);
        }
        Some(misc::Union::PermissionInfo(_pi)) => {
            debug!("SessionManager: permission info received");
        }
        Some(misc::Union::Option(_opt)) => {
            debug!("SessionManager: option message from peer");
        }
        _ => {
            debug!("SessionManager: unhandled misc type");
        }
    }
}

async fn handle_command<R: Runtime>(
    cmd: &SessionCommand,
    msg_tx: &mpsc::Sender<PeerMessage>,
    clipboard: &mut ClipboardSync,
    file_transfer: &mut FileTransferSession,
    app: &AppHandle<R>,
) {
    match cmd {
        SessionCommand::MouseMove { x, y } => {
            let msg = input_pipeline::build_mouse_move(*x, *y);
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::MouseButton { x, y, button, down, modifiers } => {
            let msg = input_pipeline::build_mouse_button(*x, *y, *button, *down, modifiers);
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::MouseWheel { x, y, delta_x, delta_y } => {
            let msg = input_pipeline::build_mouse_wheel(*x, *y, *delta_x, *delta_y);
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::Key { key, down, modifiers } => {
            if let Some(msg) = input_pipeline::build_key_event(key, *down, modifiers) {
                let _ = msg_tx.send(msg).await;
            }
        }
        SessionCommand::SpecialKey { key } => {
            let msg = match key.as_str() {
                "CtrlAltDel" => input_pipeline::build_ctrl_alt_del(),
                "LockScreen" => input_pipeline::build_lock_screen(),
                _ => return,
            };
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::ToggleClipboard { enabled } => {
            clipboard.set_enabled(*enabled);
        }
        SessionCommand::SwitchDisplay { index } => {
            let msg = PeerMessage {
                union: Some(MsgUnion::Misc(Misc {
                    union: Some(misc::Union::SwitchDisplay(SwitchDisplay {
                        display: *index,
                        x: 0,
                        y: 0,
                        width: 0,
                        height: 0,
                        cursor_embedded: false,
                        resolutions: None,
                        original_resolution: None,
                    })),
                })),
            };
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::SetQuality { image_quality, fps } => {
            let quality_enum = match image_quality.as_str() {
                "best" | "Best" => crate::proto::ImageQuality::Best,
                "balanced" | "Balanced" => crate::proto::ImageQuality::Balanced,
                "low" | "Low" => crate::proto::ImageQuality::Low,
                _ => crate::proto::ImageQuality::Best,
            };
            let msg = PeerMessage {
                union: Some(MsgUnion::Misc(Misc {
                    union: Some(misc::Union::Option(OptionMessage {
                        image_quality: quality_enum as i32,
                        custom_fps: *fps as i32,
                        ..Default::default()
                    })),
                })),
            };
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::RefreshVideo => {
            let msg = PeerMessage {
                union: Some(MsgUnion::Misc(Misc {
                    union: Some(misc::Union::RefreshVideo(true)),
                })),
            };
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::BrowseRemote { path } => {
            let msg = file_transfer.browse_remote(path);
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::DownloadFile { remote_path, filename } => {
            let msg = file_transfer.download(remote_path, filename);
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::CancelTransfer { id } => {
            let msg = file_transfer.cancel(*id);
            let _ = msg_tx.send(msg).await;
        }
        SessionCommand::ToggleRecording { enabled } => {
            let _ = app.emit("remote-recording-status", enabled);
            debug!("Recording: {}", if *enabled { "started" } else { "stopped" });
        }
        SessionCommand::Stop => {
            // Handled in the select loop
        }
    }
}

async fn send_video_received(msg_tx: &mpsc::Sender<PeerMessage>) {
    let msg = PeerMessage {
        union: Some(MsgUnion::Misc(Misc {
            union: Some(misc::Union::VideoReceived(true)),
        })),
    };
    let _ = msg_tx.send(msg).await;
}

use prost::Message;

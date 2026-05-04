//! Yomie native WebSocket relay client.
//!
//! Connects to the Yomie console relay endpoint (`/ws/bd-relay`) for
//! E2E-encrypted data transfer between two Yomie desktop clients.
//!
//! Flow:
//!   1. POST /api/bd/connect → get session_id + token
//!   2. WS  /ws/bd-relay?session=…&token=…&role=initiator
//!   3. Wait for { type: "paired" }
//!   4. Exchange E2E-encrypted binary frames

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};

use crate::config::Settings;

// ---------------------------------------------------------------------------
//  Constants
// ---------------------------------------------------------------------------

const CONNECT_TIMEOUT_SECS: u64 = 15;
const PAIR_TIMEOUT_SECS: u64 = 60;

// ---------------------------------------------------------------------------
//  API types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ConnectRequest {
    target_id: String,
    initiator_id: String,
    public_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ConnectResponse {
    success: bool,
    session_id: Option<String>,
    token: Option<String>,
    target_online: Option<bool>,
    relay_url: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

// ---------------------------------------------------------------------------
//  Relay connection
// ---------------------------------------------------------------------------

/// Active relay connection to a remote Yomie client.
pub struct BdRelayConnection {
    /// Outgoing binary frames to send to the peer.
    pub tx: mpsc::Sender<Vec<u8>>,
    /// Incoming binary frames from the peer.
    pub rx: mpsc::Receiver<Vec<u8>>,
    /// Connection status channel.
    status_rx: watch::Receiver<BdRelayStatus>,
    /// Cancel signal.
    cancel_tx: watch::Sender<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BdRelayStatus {
    pub state: BdRelayState,
    pub session_id: String,
    pub peer_id: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum BdRelayState {
    Connecting,
    WaitingForPeer,
    Paired,
    Active,
    Closed,
    Error,
}

impl BdRelayConnection {
    /// Initiate a relay connection to a target device.
    ///
    /// 1. Calls POST /api/bd/connect to create a relay session
    /// 2. Opens WebSocket to /ws/bd-relay
    /// 3. Waits for pairing
    /// 4. Returns channels for bidirectional binary data
    ///
    /// `base_url` is the console URL (e.g. `http://192.168.0.110:5000`),
    ///     passed from the frontend to match the origin of session cookies.
    /// `http_client` is the shared reqwest client with cookie store from AppState.
    /// If `access_token` is available, it's sent as Bearer auth.
    /// Otherwise the session cookies from prior `api_proxy` login are used.
    pub async fn connect(
        base_url: &str,
        settings: &Settings,
        http_client: &reqwest::Client,
        my_id: &str,
        target_id: &str,
    ) -> Result<Self> {
        let base_url = base_url.trim_end_matches('/');
        let token = settings.access_token.clone().unwrap_or_default();

        // Step 1: Request relay session via HTTP
        let connect_url = format!("{}/api/bd/connect", base_url);
        info!("[bd_relay] POST {} (target={}, initiator={}, token_len={}, auth={})",
            connect_url, target_id, my_id, token.len(),
            if !token.is_empty() { "Bearer" } else { "cookies" });

        let body = ConnectRequest {
            target_id: target_id.to_string(),
            initiator_id: my_id.to_string(),
            public_key: None, // TODO: E2E key exchange
        };

        let mut req_builder = http_client
            .post(&connect_url)
            .timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .header("Content-Type", "application/json")
            .header("Origin", "https://tauri.localhost")
            .json(&body);

        // Attach Bearer token if available; otherwise session cookies are sent automatically
        if !token.is_empty() {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", token));
        }

        let resp = req_builder
            .send()
            .await
            .map_err(|e| {
                log::error!("[bd_relay] HTTP POST failed: {} (url={})", e, connect_url);
                anyhow::anyhow!("Failed to request relay session: {}", e)
            })?;

        let status = resp.status();
        info!("[bd_relay] POST {} → HTTP {}", connect_url, status);

        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            log::error!("[bd_relay] Connect request failed: HTTP {} — {}", status, &text[..text.len().min(500)]);
            bail!("Connect request failed (HTTP {}): {}", status, text);
        }

        let connect_resp: ConnectResponse = resp.json().await?;

        if !connect_resp.success {
            bail!(
                "Server rejected connection: {}",
                connect_resp.error.unwrap_or_else(|| "Unknown error".into())
            );
        }

        let session_id = connect_resp
            .session_id
            .context("Missing session_id in response")?;
        let relay_token = connect_resp
            .token
            .context("Missing token in response")?;

        info!(
            "Relay session {} created (target_online: {:?})",
            session_id,
            connect_resp.target_online
        );

        // Step 2: Build WebSocket URL
        let ws_url = build_ws_url(&base_url, &session_id, &relay_token, "initiator")?;
        info!("Connecting WebSocket: {}", ws_url);

        // Step 3: Connect WebSocket
        let (ws_stream, _) = tokio::time::timeout(
            std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS),
            connect_async(&ws_url),
        )
        .await
        .context("WebSocket connection timeout")?
        .context("WebSocket connection failed")?;

        info!("WebSocket connected to relay");

        // Set up channels
        let (outgoing_tx, outgoing_rx) = mpsc::channel::<Vec<u8>>(256);
        let (incoming_tx, incoming_rx) = mpsc::channel::<Vec<u8>>(256);
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let initial_status = BdRelayStatus {
            state: BdRelayState::WaitingForPeer,
            session_id: session_id.clone(),
            peer_id: target_id.to_string(),
            bytes_sent: 0,
            bytes_received: 0,
            last_error: None,
        };
        let (status_tx, status_rx) = watch::channel(initial_status);

        // Spawn relay task
        let peer_id = target_id.to_string();
        let sid = session_id.clone();
        tauri::async_runtime::spawn(async move {
            relay_task(ws_stream, outgoing_rx, incoming_tx, status_tx, cancel_rx, sid, peer_id)
                .await;
        });

        Ok(BdRelayConnection {
            tx: outgoing_tx,
            rx: incoming_rx,
            status_rx,
            cancel_tx,
        })
    }

    /// Accept an incoming relay connection (target side).
    pub async fn accept(
        settings: &Settings,
        session_id: &str,
        relay_token: &str,
        peer_id: &str,
    ) -> Result<Self> {
        let base_url = settings.bd_api_url();

        let ws_url = build_ws_url(&base_url, session_id, relay_token, "target")?;
        info!("Accepting relay connection: {}", ws_url);

        let (ws_stream, _) = tokio::time::timeout(
            std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS),
            connect_async(&ws_url),
        )
        .await
        .context("WebSocket connection timeout")?
        .context("WebSocket connection failed")?;

        let (outgoing_tx, outgoing_rx) = mpsc::channel::<Vec<u8>>(256);
        let (incoming_tx, incoming_rx) = mpsc::channel::<Vec<u8>>(256);
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let initial_status = BdRelayStatus {
            state: BdRelayState::WaitingForPeer,
            session_id: session_id.to_string(),
            peer_id: peer_id.to_string(),
            bytes_sent: 0,
            bytes_received: 0,
            last_error: None,
        };
        let (status_tx, status_rx) = watch::channel(initial_status);

        let sid = session_id.to_string();
        let pid = peer_id.to_string();
        tauri::async_runtime::spawn(async move {
            relay_task(ws_stream, outgoing_rx, incoming_tx, status_tx, cancel_rx, sid, pid).await;
        });

        Ok(BdRelayConnection {
            tx: outgoing_tx,
            rx: incoming_rx,
            status_rx,
            cancel_tx,
        })
    }

    pub fn status(&self) -> BdRelayStatus {
        self.status_rx.borrow().clone()
    }

    pub fn close(&self) {
        let _ = self.cancel_tx.send(true);
    }
}

// ---------------------------------------------------------------------------
//  Relay WebSocket task
// ---------------------------------------------------------------------------

async fn relay_task(
    ws_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    mut outgoing_rx: mpsc::Receiver<Vec<u8>>,
    incoming_tx: mpsc::Sender<Vec<u8>>,
    status_tx: watch::Sender<BdRelayStatus>,
    mut cancel_rx: watch::Receiver<bool>,
    session_id: String,
    peer_id: String,
) {
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    let mut bytes_sent: u64 = 0;
    let mut bytes_received: u64 = 0;

    // Wait for pairing message first
    let pair_deadline =
        tokio::time::Instant::now() + tokio::time::Duration::from_secs(PAIR_TIMEOUT_SECS);

    loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                            if parsed.get("type").and_then(|v| v.as_str()) == Some("paired") {
                                info!("Relay session {} paired!", session_id);
                                let _ = status_tx.send(BdRelayStatus {
                                    state: BdRelayState::Paired,
                                    session_id: session_id.clone(),
                                    peer_id: peer_id.clone(),
                                    bytes_sent,
                                    bytes_received,
                                    last_error: None,
                                });
                                break;
                            }
                        }
                    }
                    Some(Ok(WsMessage::Binary(data))) => {
                        // Binary before pairing — unexpected but forward anyway
                        bytes_received += data.len() as u64;
                        let _ = incoming_tx.send(data.to_vec()).await;
                    }
                    Some(Ok(WsMessage::Close(_))) | None => {
                        warn!("WebSocket closed before pairing");
                        let _ = status_tx.send(BdRelayStatus {
                            state: BdRelayState::Closed,
                            session_id: session_id.clone(),
                            peer_id: peer_id.clone(),
                            bytes_sent,
                            bytes_received,
                            last_error: Some("Closed before pairing".into()),
                        });
                        return;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error during pairing: {}", e);
                        let _ = status_tx.send(BdRelayStatus {
                            state: BdRelayState::Error,
                            session_id: session_id.clone(),
                            peer_id: peer_id.clone(),
                            bytes_sent,
                            bytes_received,
                            last_error: Some(e.to_string()),
                        });
                        return;
                    }
                    _ => {}
                }
            }
            _ = tokio::time::sleep_until(pair_deadline) => {
                warn!("Pairing timeout for session {}", session_id);
                let _ = status_tx.send(BdRelayStatus {
                    state: BdRelayState::Closed,
                    session_id: session_id.clone(),
                    peer_id: peer_id.clone(),
                    bytes_sent,
                    bytes_received,
                    last_error: Some("Pairing timeout".into()),
                });
                return;
            }
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    info!("Relay cancelled during pairing");
                    let _ = ws_tx.close().await;
                    return;
                }
            }
        }
    }

    // Active data relay loop
    let _ = status_tx.send(BdRelayStatus {
        state: BdRelayState::Active,
        session_id: session_id.clone(),
        peer_id: peer_id.clone(),
        bytes_sent,
        bytes_received,
        last_error: None,
    });

    loop {
        tokio::select! {
            // Incoming from WebSocket → forward to app
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(WsMessage::Binary(data))) => {
                        bytes_received += data.len() as u64;
                        if incoming_tx.send(data.to_vec()).await.is_err() {
                            info!("Incoming channel closed, ending relay");
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Text(text))) => {
                        // Control messages during active relay
                        debug!("Relay control message: {}", text);
                    }
                    Some(Ok(WsMessage::Close(_))) | None => {
                        info!("Peer disconnected from relay");
                        break;
                    }
                    Some(Ok(WsMessage::Ping(data))) => {
                        let _ = ws_tx.send(WsMessage::Pong(data)).await;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    _ => {}
                }
            }

            // Outgoing from app → forward to WebSocket
            data = outgoing_rx.recv() => {
                match data {
                    Some(frame) => {
                        bytes_sent += frame.len() as u64;
                        if ws_tx.send(WsMessage::Binary(frame.into())).await.is_err() {
                            warn!("Failed to send to WebSocket");
                            break;
                        }
                    }
                    None => {
                        info!("Outgoing channel closed, ending relay");
                        break;
                    }
                }
            }

            // Cancellation
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    info!("Relay cancelled");
                    break;
                }
            }
        }
    }

    // Cleanup
    let _ = ws_tx.close().await;
    let _ = status_tx.send(BdRelayStatus {
        state: BdRelayState::Closed,
        session_id: session_id.clone(),
        peer_id: peer_id.clone(),
        bytes_sent,
        bytes_received,
        last_error: None,
    });
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

fn build_ws_url(
    base_url: &str,
    session_id: &str,
    token: &str,
    role: &str,
) -> Result<String> {
    // Convert http(s) base URL to ws(s)
    let ws_base = if base_url.starts_with("https://") {
        base_url.replacen("https://", "wss://", 1)
    } else if base_url.starts_with("http://") {
        base_url.replacen("http://", "ws://", 1)
    } else {
        format!("ws://{}", base_url)
    };

    // Remove trailing slash
    let ws_base = ws_base.trim_end_matches('/');

    Ok(format!(
        "{}/ws/bd-relay?session={}&token={}&role={}",
        ws_base, session_id, token, role
    ))
}

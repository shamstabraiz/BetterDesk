//! WebSocket connection to the CDAP gateway.
//!
//! Handles connect, authenticate (3 methods), register, send/recv, and
//! heartbeat. Built on `tokio-tungstenite` for async I/O.

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use log::{debug, info};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
    MaybeTlsStream, WebSocketStream,
};

use super::protocol::{
    CdapMessage, DeviceManifest, MessagePayload, SystemMetrics,
};
use super::CdapConfig;

/// Active WebSocket connection to the CDAP gateway.
///
/// After `connect()` + `authenticate()` + `register_manifest()`, call
/// `into_split()` to get a `CdapSender` + `CdapReceiver` pair that can
/// be used concurrently without locking.
pub struct CdapConnection {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

/// Write half — safe to use from multiple tasks via `mpsc::Sender<CdapMessage>`.
pub struct CdapSender {
    tx: futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
}

/// Read half — must be driven by exactly one task.
pub struct CdapReceiver {
    rx: futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl CdapConnection {
    /// Establish a WebSocket connection to the gateway URL in `config`.
    pub async fn connect(config: &CdapConfig) -> Result<Self> {
        let url = &config.gateway_url;
        if url.is_empty() {
            bail!("Gateway URL is empty");
        }

        let (ws, _response) = tokio::time::timeout(
            std::time::Duration::from_secs(15),
            connect_async(url),
        )
        .await
        .map_err(|_| anyhow::anyhow!("WebSocket connect timed out after 15s"))?
        .context("WebSocket connect failed")?;

        info!("CDAP-conn: WebSocket connected to {}", url);
        Ok(Self { ws })
    }

    // ------------------------------------------------------------------
    //  Phase 1 — Authentication
    // ------------------------------------------------------------------

    pub async fn authenticate(&mut self, config: &CdapConfig) -> Result<AuthResult> {
        let msg = match config.auth_method.as_str() {
            "api_key" => CdapMessage::auth(
                "api_key",
                &config.device_id,
                Some(&config.api_key),
                None,
                None,
                None,
            ),
            "device_token" => CdapMessage::auth(
                "device_token",
                &config.device_id,
                None,
                Some(&config.device_token),
                None,
                None,
            ),
            "user_password" => CdapMessage::auth(
                "user_password",
                &config.device_id,
                None,
                None,
                Some(&config.username),
                Some(&config.password),
            ),
            other => bail!("Unknown auth method: {}", other),
        };

        self.send_message(&msg).await?;

        // Wait for auth_result
        let response = self
            .recv_message()
            .await?
            .context("Connection closed during auth")?;

        if response.msg_type != "auth_result" {
            bail!("Expected auth_result, got: {}", response.msg_type);
        }

        // Parse auth_result payload
        match &response.payload {
            MessagePayload::AuthResult {
                success,
                token,
                role,
                device_id,
                error,
                ..
            } => {
                if !success {
                    let err_msg = error.as_deref().unwrap_or("unknown error");
                    bail!("Authentication rejected: {}", err_msg);
                }
                Ok(AuthResult {
                    token: token.clone().unwrap_or_default(),
                    role: role.clone().unwrap_or_default(),
                    device_id: device_id.clone().unwrap_or_default(),
                })
            }
            // serde untagged may deserialize into Generic
            MessagePayload::Generic(v) => {
                let success = v.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                if !success {
                    let err_msg = v
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    bail!("Authentication rejected: {}", err_msg);
                }
                Ok(AuthResult {
                    token: v
                        .get("token")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    role: v
                        .get("role")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                    device_id: v
                        .get("device_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .into(),
                })
            }
            _ => bail!("Unexpected auth_result payload shape"),
        }
    }

    // ------------------------------------------------------------------
    //  Phase 2 — Registration
    // ------------------------------------------------------------------

    pub async fn register_manifest(&mut self, manifest: &DeviceManifest) -> Result<()> {
        let msg = CdapMessage::register(manifest);
        self.send_message(&msg).await?;
        debug!("CDAP-conn: Manifest registered");
        Ok(())
    }

    // ------------------------------------------------------------------
    //  Heartbeat
    // ------------------------------------------------------------------

    pub async fn send_heartbeat(
        &mut self,
        metrics: &HeartbeatPayload,
    ) -> Result<()> {
        let msg = CdapMessage::heartbeat(
            metrics.metrics.clone(),
            metrics.widget_values.clone(),
        );
        self.send_message(&msg).await
    }

    // ------------------------------------------------------------------
    //  Low-level send / recv
    // ------------------------------------------------------------------

    pub async fn send_message(&mut self, msg: &CdapMessage) -> Result<()> {
        let json = serde_json::to_string(msg).context("Serialize message")?;
        debug!("CDAP-conn TX: {}", &json[..json.len().min(200)]);
        self.ws
            .send(Message::Text(json))
            .await
            .context("WebSocket send")?;
        Ok(())
    }

    pub async fn recv_message(&mut self) -> Result<Option<CdapMessage>> {
        loop {
            match self.ws.next().await {
                Some(Ok(Message::Text(text))) => {
                    debug!("CDAP-conn RX: {}", &text[..text.len().min(200)]);
                    let msg: CdapMessage =
                        serde_json::from_str(&text).context("Parse CDAP message")?;
                    return Ok(Some(msg));
                }
                Some(Ok(Message::Ping(data))) => {
                    let _ = self.ws.send(Message::Pong(data)).await;
                    continue;
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Ok(None);
                }
                Some(Ok(_)) => {
                    // Binary or other — skip
                    continue;
                }
                Some(Err(e)) => {
                    return Err(e).context("WebSocket recv error");
                }
            }
        }
    }

    /// Graceful close.
    pub async fn close(&mut self) {
        let _ = self
            .ws
            .send(Message::Close(Some(tungstenite::protocol::CloseFrame {
                code: tungstenite::protocol::frame::coding::CloseCode::Normal,
                reason: "agent shutdown".into(),
            })))
            .await;
    }

    /// Split the connection into a sender and receiver half.
    ///
    /// After calling this, use `CdapSender::send_message()` from any task
    /// and drive `CdapReceiver::recv_message()` from a single reader task.
    /// This avoids holding a write lock across `.await`.
    pub fn into_split(self) -> (CdapSender, CdapReceiver) {
        let (tx, rx) = self.ws.split();
        (CdapSender { tx }, CdapReceiver { rx })
    }
}

impl CdapSender {
    pub async fn send_message(&mut self, msg: &CdapMessage) -> Result<()> {
        let json = serde_json::to_string(msg).context("Serialize message")?;
        debug!("CDAP-conn TX: {}", &json[..json.len().min(200)]);
        self.tx
            .send(Message::Text(json))
            .await
            .context("WebSocket send")?;
        Ok(())
    }

    pub async fn send_heartbeat(&mut self, metrics: &HeartbeatPayload) -> Result<()> {
        let msg = CdapMessage::heartbeat(
            metrics.metrics.clone(),
            metrics.widget_values.clone(),
        );
        self.send_message(&msg).await
    }

    pub async fn close(&mut self) {
        let _ = self
            .tx
            .send(Message::Close(Some(tungstenite::protocol::CloseFrame {
                code: tungstenite::protocol::frame::coding::CloseCode::Normal,
                reason: "agent shutdown".into(),
            })))
            .await;
    }
}

impl CdapReceiver {
    pub async fn recv_message(&mut self) -> Result<Option<CdapMessage>> {
        loop {
            match self.rx.next().await {
                Some(Ok(Message::Text(text))) => {
                    debug!("CDAP-conn RX: {}", &text[..text.len().min(200)]);
                    let msg: CdapMessage =
                        serde_json::from_str(&text).context("Parse CDAP message")?;
                    return Ok(Some(msg));
                }
                Some(Ok(Message::Ping(_))) => {
                    // Pong handled automatically by tungstenite in split mode
                    continue;
                }
                Some(Ok(Message::Close(_))) | None => {
                    return Ok(None);
                }
                Some(Ok(_)) => {
                    continue;
                }
                Some(Err(e)) => {
                    return Err(e).context("WebSocket recv error");
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
//  Supporting types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AuthResult {
    pub token: String,
    pub role: String,
    pub device_id: String,
}

#[derive(Debug, Clone)]
pub struct HeartbeatPayload {
    pub metrics: SystemMetrics,
    pub widget_values: Option<std::collections::HashMap<String, Value>>,
}

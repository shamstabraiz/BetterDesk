//! Peer session — state machine for a remote desktop connection.
//!
//! Lifecycle:
//! 1. `New`           — created, not yet connected
//! 2. `Connecting`    — signal server negotiation
//! 3. `Authenticating`— relay connected, waiting for password
//! 4. `Connected`     — authenticated, streaming
//! 5. `Disconnected`  — closed

use anyhow::{bail, Context, Result};
use log::{debug, info, warn, error};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

use crate::commands::ConnectionStatus;
use crate::config::Settings;
use crate::crypto::hash_password;
use crate::network::relay::{AuthChallenge, RelayConnection};
use crate::network::signal::{self, PunchResult};
use crate::proto::{
    message::Union as MsgUnion, PeerInfo,
};

/// Connection state enum.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum ConnectionState {
    Idle,
    Connecting,
    Authenticating,
    Connected,
    Disconnected,
    Error(String),
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Idle => write!(f, "idle"),
            ConnectionState::Connecting => write!(f, "connecting"),
            ConnectionState::Authenticating => write!(f, "authenticating"),
            ConnectionState::Connected => write!(f, "connected"),
            ConnectionState::Disconnected => write!(f, "disconnected"),
            ConnectionState::Error(e) => write!(f, "error: {}", e),
        }
    }
}

/// A peer session managing the full connection lifecycle.
pub struct Session {
    state: Arc<Mutex<SessionInner>>,
    settings: Settings,
    peer_id: String,
}

struct SessionInner {
    state: ConnectionState,
    relay: Option<RelayConnection>,
    auth_challenge: Option<AuthChallenge>,
    peer_info: Option<PeerInfo>,
    last_latency_ms: Option<u32>,
    disconnect_tx: Option<mpsc::Sender<()>>,
}

impl Session {
    /// Create a cheap clone of this session handle (shares the same inner state via Arc).
    pub fn clone_handle(&self) -> Self {
        Session {
            state: Arc::clone(&self.state),
            settings: self.settings.clone(),
            peer_id: self.peer_id.clone(),
        }
    }

    /// Create a new session (starts connection in the background).
    pub fn new(settings: &Settings, peer_id: &str) -> Result<Self> {
        let session = Session {
            state: Arc::new(Mutex::new(SessionInner {
                state: ConnectionState::Connecting,
                relay: None,
                auth_challenge: None,
                peer_info: None,
                last_latency_ms: None,
                disconnect_tx: None,
            })),
            settings: settings.clone(),
            peer_id: peer_id.to_string(),
        };

        // Spawn connection task
        let state_clone = session.state.clone();
        let settings_clone = session.settings.clone();
        let peer_id_clone = session.peer_id.clone();

        tokio::spawn(async move {
            if let Err(e) = Self::connect_task(state_clone.clone(), &settings_clone, &peer_id_clone).await {
                error!("Connection failed: {}", e);
                let mut inner = state_clone.lock().await;
                inner.state = ConnectionState::Error(e.to_string());
            }
        });

        Ok(session)
    }

    /// Internal connection task.
    async fn connect_task(
        state: Arc<Mutex<SessionInner>>,
        settings: &Settings,
        peer_id: &str,
    ) -> Result<()> {
        let my_id = crate::identity::get_or_create_device_id()?;
        let signal_addr = settings.hbbs_address();

        // Step 1: Punch hole via signal server
        info!("[SESSION] Starting punch hole to {} (server={})", peer_id, signal_addr);
        let result = signal::punch_hole(
            &signal_addr,
            &my_id,
            peer_id,
            settings.force_relay,
        )
        .await?;

        match result {
            PunchResult::Relay {
                relay_server,
                peer_pk,
                uuid,
            } => {
                info!("[SESSION] Got relay: {} (uuid={})", relay_server, uuid);

                // Step 2: Connect to relay
                let relay_addr = if relay_server.contains(':') {
                    relay_server.clone()
                } else {
                    format!("{}:21117", relay_server)
                };

                info!("[SESSION] Relay addr resolved to: {}", relay_addr);

                let (relay_conn, auth_challenge) =
                    RelayConnection::connect(&relay_addr, &my_id, peer_id, &uuid, &peer_pk)
                        .await?;

                info!("[SESSION] Relay connected, state -> Authenticating");

                // Update state to authenticating
                let mut inner = state.lock().await;
                inner.state = ConnectionState::Authenticating;
                inner.relay = Some(relay_conn);
                inner.auth_challenge = Some(auth_challenge);

                info!("[SESSION] Ready for authentication");
            }
            PunchResult::Direct { .. } => {
                // Direct UDP connections are not yet supported
                bail!("Direct P2P connections are not yet implemented — use force_relay=true");
            }
            PunchResult::Failure(reason) => {
                bail!("Peer connection failed: {}", reason);
            }
        }

        Ok(())
    }

    /// Authenticate with the peer using a password.
    pub async fn authenticate(&self, password: &str) -> Result<()> {
        let mut inner = self.state.lock().await;
        info!("[SESSION] Authenticate called, current state={}", inner.state);

        if inner.state != ConnectionState::Authenticating {
            bail!("Cannot authenticate in state: {}", inner.state);
        }

        let challenge = inner
            .auth_challenge
            .take()
            .context("No auth challenge available")?;

        let password_hash = hash_password(password, &challenge.salt, &challenge.challenge);
        info!("[SESSION] Password hashed (salt_len={}, challenge_len={})", challenge.salt.len(), challenge.challenge.len());

        let my_id = crate::identity::get_or_create_device_id()?;
        info!("[SESSION] Sending LoginRequest (my_id={})", my_id);

        let relay = inner
            .relay
            .as_mut()
            .context("No relay connection")?;

        // Send LoginRequest
        relay.send_login(&my_id, &password_hash).await?;

        // Wait for LoginResponse
        info!("[SESSION] Waiting for LoginResponse...");
        let response = relay.read_message().await?;
        info!("[SESSION] Got response: {:?}", response.union.as_ref().map(|u| std::mem::discriminant(u)));

        match response.union {
            Some(MsgUnion::LoginResponse(lr)) => {
                match lr.union {
                    Some(crate::proto::login_response::Union::PeerInfo(peer_info)) => {
                        info!(
                            "Authenticated! peer={} platform={} version={}",
                            peer_info.hostname, peer_info.platform, peer_info.version
                        );
                        inner.peer_info = Some(peer_info);
                        inner.state = ConnectionState::Connected;
                    }
                    Some(crate::proto::login_response::Union::Error(err)) => {
                        warn!("Authentication failed: {}", err);
                        inner.state = ConnectionState::Error(format!("Auth failed: {}", err));
                        bail!("Authentication failed: {}", err);
                    }
                    None => {
                        bail!("Empty LoginResponse");
                    }
                }
            }
            Some(MsgUnion::TestDelay(_)) => {
                // TestDelay can arrive before LoginResponse — read again
                debug!("Skipped TestDelay, waiting for LoginResponse");
                let response2 = relay.read_message().await?;
                if let Some(MsgUnion::LoginResponse(lr)) = response2.union {
                    match lr.union {
                        Some(crate::proto::login_response::Union::PeerInfo(peer_info)) => {
                            info!("Authenticated (after TestDelay skip)!");
                            inner.peer_info = Some(peer_info);
                            inner.state = ConnectionState::Connected;
                        }
                        Some(crate::proto::login_response::Union::Error(err)) => {
                            inner.state = ConnectionState::Error(format!("Auth failed: {}", err));
                            bail!("Authentication failed: {}", err);
                        }
                        None => bail!("Empty LoginResponse"),
                    }
                } else {
                    bail!("Expected LoginResponse after TestDelay");
                }
            }
            other => {
                bail!("Unexpected message after login: {:?}", other);
            }
        }

        Ok(())
    }

    /// Send a key event to the peer.
    pub fn send_key(&self, event: &crate::commands::KeyEventPayload) -> Result<()> {
        // TODO: Build KeyEvent protobuf and send via relay
        debug!("Key event: {:?}", event);
        Ok(())
    }

    /// Send a mouse event to the peer.
    pub fn send_mouse(&self, event: &crate::commands::MouseEventPayload) -> Result<()> {
        // TODO: Build MouseEvent protobuf and send via relay
        debug!("Mouse event: {:?}", event);
        Ok(())
    }

    /// Take the relay connection out of the session (consumes it).
    ///
    /// Used by `start_remote_session` to bridge the relay into `SessionManager`.
    /// After this call the session enters `Connected` state but no longer owns
    /// the relay — all I/O goes through the `SessionManager` channels.
    pub async fn take_relay(&self) -> Option<RelayConnection> {
        let mut inner = self.state.lock().await;
        inner.relay.take()
    }

    /// Disconnect the session.
    pub fn disconnect(&self) {
        let state = self.state.clone();
        tokio::spawn(async move {
            let mut inner = state.lock().await;
            inner.state = ConnectionState::Disconnected;
            inner.relay = None;
            inner.auth_challenge = None;
            if let Some(tx) = inner.disconnect_tx.take() {
                let _ = tx.send(()).await;
            }
            info!("Session disconnected");
        });
    }

    /// Get current connection status for the frontend.
    pub fn status(&self) -> ConnectionStatus {
        // Try to get lock without blocking
        let inner = self.state.try_lock();
        match inner {
            Ok(inner) => ConnectionStatus {
                state: format!("{}", inner.state),
                peer_id: Some(self.peer_id.clone()),
                peer_info: inner.peer_info.as_ref().map(|pi| {
                    serde_json::json!({
                        "hostname": pi.hostname,
                        "platform": pi.platform,
                        "username": pi.username,
                        "version": pi.version,
                        "displays": pi.displays.len(),
                    })
                }),
                latency_ms: inner.last_latency_ms,
                error: match &inner.state {
                    ConnectionState::Error(e) => Some(e.clone()),
                    _ => None,
                },
            },
            Err(_) => ConnectionStatus {
                state: "busy".into(),
                peer_id: Some(self.peer_id.clone()),
                peer_info: None,
                latency_ms: None,
                error: None,
            },
        }
    }
}

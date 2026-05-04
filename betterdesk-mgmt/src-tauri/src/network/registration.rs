//! Registration service — persistent UDP heartbeat to hbbs.
//!
//! Maintains a UDP "connection" to the signal server, sends RegisterPeer
//! heartbeats every 12 seconds, and handles RegisterPk when the server
//! requests our public key.  This keeps the client visible in the web
//! panel device list.
//!
//! The RustDesk rendezvous protocol uses UDP for registration heartbeats
//! and TCP only for signaling (PunchHole, relay negotiation).

use anyhow::{Context, Result};
use log::{debug, info, warn};
use prost::Message;
use tokio::net::UdpSocket;
use tokio::sync::watch;
use tokio::time::{sleep, Duration, Instant};

use crate::config::Settings;
use crate::proto::{
    rendezvous_message::Union as RdzUnion, RegisterPeer, RegisterPk, RendezvousMessage,
};

/// Heartbeat interval — matches the server's `HeartbeatSuggestion` (12s).
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(12);

/// Reconnect delay after a failure.
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// UDP receive timeout — how long to wait for a server response.
const UDP_RECV_TIMEOUT: Duration = Duration::from_secs(3);

/// Registration status exposed to the frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RegistrationStatus {
    /// Whether the client is currently registered with hbbs.
    pub registered: bool,
    /// Current device ID.
    pub device_id: String,
    /// Server address we're connected to (or trying to connect to).
    pub server_address: String,
    /// Number of successful heartbeats sent.
    pub heartbeat_count: u64,
    /// Last error message (if any).
    pub last_error: Option<String>,
}

/// Handle to the registration service — used to query status and stop it.
pub struct RegistrationService {
    status_rx: watch::Receiver<RegistrationStatus>,
    cancel_tx: watch::Sender<bool>,
}

impl RegistrationService {
    /// Start the registration service in the background.
    ///
    /// Returns a handle that can be used to query status and stop the service.
    pub fn start(settings: &Settings, device_id: &str) -> Self {
        let server_address = settings.hbbs_address();

        let initial_status = RegistrationStatus {
            registered: false,
            device_id: device_id.to_string(),
            server_address: server_address.clone(),
            heartbeat_count: 0,
            last_error: None,
        };

        let (status_tx, status_rx) = watch::channel(initial_status);
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let device_id = device_id.to_string();

        tauri::async_runtime::spawn(async move {
            registration_loop(server_address, device_id, status_tx, cancel_rx).await;
        });

        RegistrationService {
            status_rx,
            cancel_tx,
        }
    }

    /// Get the current registration status.
    pub fn status(&self) -> RegistrationStatus {
        self.status_rx.borrow().clone()
    }

    /// Stop the registration service.
    pub fn stop(&self) {
        let _ = self.cancel_tx.send(true);
        info!("Registration service stop requested");
    }
}

// --------------------------------------------------------------------------
//  Internals
// --------------------------------------------------------------------------

/// Encode a protobuf message for UDP (no framing needed — each datagram is
/// one message).
fn encode_udp_msg<M: Message>(msg: &M) -> Vec<u8> {
    msg.encode_to_vec()
}

/// Main registration loop — reconnects on failure, sends heartbeats.
async fn registration_loop(
    server_address: String,
    device_id: String,
    status_tx: watch::Sender<RegistrationStatus>,
    mut cancel_rx: watch::Receiver<bool>,
) {
    let mut serial: i32 = 0;
    let mut heartbeat_count: u64 = 0;

    loop {
        // Check if cancelled
        if *cancel_rx.borrow() {
            info!("Registration service cancelled");
            let _ = status_tx.send(RegistrationStatus {
                registered: false,
                device_id: device_id.clone(),
                server_address: server_address.clone(),
                heartbeat_count,
                last_error: Some("Service stopped".into()),
            });
            return;
        }

        info!(
            "Starting UDP registration to {} as {}...",
            server_address, device_id
        );

        match run_udp_heartbeat(
            &server_address,
            &device_id,
            &mut serial,
            &mut heartbeat_count,
            &status_tx,
            &mut cancel_rx,
        )
        .await
        {
            Ok(()) => {
                info!("Registration loop ended gracefully");
            }
            Err(e) => {
                warn!("Registration failed: {}", e);
                let _ = status_tx.send(RegistrationStatus {
                    registered: false,
                    device_id: device_id.clone(),
                    server_address: server_address.clone(),
                    heartbeat_count,
                    last_error: Some(e.to_string()),
                });
            }
        }

        // Check cancellation before reconnecting
        if *cancel_rx.borrow() {
            return;
        }

        info!(
            "Retrying registration in {} seconds...",
            RECONNECT_DELAY.as_secs()
        );

        tokio::select! {
            _ = sleep(RECONNECT_DELAY) => {}
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    return;
                }
            }
        }
    }
}

/// Bind a UDP socket and run the heartbeat loop.
async fn run_udp_heartbeat(
    server_address: &str,
    device_id: &str,
    serial: &mut i32,
    heartbeat_count: &mut u64,
    status_tx: &watch::Sender<RegistrationStatus>,
    cancel_rx: &mut watch::Receiver<bool>,
) -> Result<()> {
    // Bind to any available local port
    let socket = UdpSocket::bind("0.0.0.0:0")
        .await
        .context("Failed to bind UDP socket")?;

    // "Connect" to the server (sets default send target and filters incoming)
    socket
        .connect(server_address)
        .await
        .context("Failed to set UDP peer address")?;

    info!("UDP socket bound, sending to {}", server_address);

    // Send initial RegisterPeer
    let reg_msg = RendezvousMessage {
        union: Some(RdzUnion::RegisterPeer(RegisterPeer {
            id: device_id.to_string(),
            serial: *serial,
        })),
    };
    socket.send(&encode_udp_msg(&reg_msg)).await?;
    *serial += 1;
    debug!("Sent initial RegisterPeer (serial={})", *serial);

    // Wait for RegisterPeerResponse
    let mut recv_buf = vec![0u8; 2048];
    let mut request_pk = false;

    match tokio::time::timeout(UDP_RECV_TIMEOUT, socket.recv(&mut recv_buf)).await {
        Ok(Ok(n)) => {
            if let Ok(resp) = RendezvousMessage::decode(&recv_buf[..n]) {
                if let Some(RdzUnion::RegisterPeerResponse(rpr)) = resp.union {
                    request_pk = rpr.request_pk;
                    info!("RegisterPeerResponse received: request_pk={}", request_pk);
                }
            }
        }
        Ok(Err(e)) => {
            warn!("Failed to receive RegisterPeerResponse: {}", e);
        }
        Err(_) => {
            // Timeout is acceptable — some server versions don't respond
            // to every RegisterPeer. We'll keep sending heartbeats.
            debug!("No RegisterPeerResponse within timeout (OK for UDP)");
        }
    }

    // If the server wants our public key, send RegisterPk
    if request_pk {
        send_register_pk_udp(&socket, device_id).await?;

        // Optionally wait for RegisterPkResponse
        match tokio::time::timeout(UDP_RECV_TIMEOUT, socket.recv(&mut recv_buf)).await {
            Ok(Ok(n)) => {
                if let Ok(resp) = RendezvousMessage::decode(&recv_buf[..n]) {
                    if let Some(RdzUnion::RegisterPkResponse(rpkr)) = resp.union {
                        info!(
                            "RegisterPkResponse: result={}, keep_alive={}",
                            rpkr.result, rpkr.keep_alive
                        );
                    }
                }
            }
            _ => {
                debug!("No RegisterPkResponse within timeout");
            }
        }
    }

    // We're registered — update status
    *heartbeat_count += 1;
    let _ = status_tx.send(RegistrationStatus {
        registered: true,
        device_id: device_id.to_string(),
        server_address: server_address.to_string(),
        heartbeat_count: *heartbeat_count,
        last_error: None,
    });

    info!(
        "Registered with {} as {} (heartbeats: {})",
        server_address, device_id, heartbeat_count
    );

    // ---- Heartbeat loop ----
    let mut last_heartbeat = Instant::now();
    let mut consecutive_failures: u32 = 0;

    loop {
        tokio::select! {
            // Send heartbeat every HEARTBEAT_INTERVAL
            _ = sleep(HEARTBEAT_INTERVAL.saturating_sub(last_heartbeat.elapsed())) => {
                let hb_msg = RendezvousMessage {
                    union: Some(RdzUnion::RegisterPeer(RegisterPeer {
                        id: device_id.to_string(),
                        serial: *serial,
                    })),
                };

                match socket.send(&encode_udp_msg(&hb_msg)).await {
                    Ok(_) => {
                        *serial += 1;
                        *heartbeat_count += 1;
                        last_heartbeat = Instant::now();
                        consecutive_failures = 0;

                        debug!(
                            "Heartbeat sent (serial={}, total={})",
                            *serial, *heartbeat_count
                        );

                        let _ = status_tx.send(RegistrationStatus {
                            registered: true,
                            device_id: device_id.to_string(),
                            server_address: server_address.to_string(),
                            heartbeat_count: *heartbeat_count,
                            last_error: None,
                        });
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        warn!("Failed to send heartbeat: {} (failures: {})", e, consecutive_failures);
                        if consecutive_failures >= 5 {
                            return Err(e.into());
                        }
                    }
                }
            }

            // Process any incoming UDP responses (RegisterPeerResponse
            // echoed back after heartbeats, HealthCheck, etc.)
            result = socket.recv(&mut recv_buf) => {
                match result {
                    Ok(n) if n > 0 => {
                        if let Ok(msg) = RendezvousMessage::decode(&recv_buf[..n]) {
                            handle_server_message_udp(&socket, device_id, msg).await;
                        }
                    }
                    Ok(_) => {} // Empty datagram, ignore
                    Err(e) => {
                        debug!("UDP recv error (non-fatal): {}", e);
                    }
                }
            }

            // Check for cancellation
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    info!("Heartbeat loop cancelled");
                    return Ok(());
                }
            }
        }
    }
}

/// Send RegisterPk over UDP with our Ed25519 public key and UUID.
async fn send_register_pk_udp(socket: &UdpSocket, device_id: &str) -> Result<()> {
    let (pk_bytes, uuid_bytes) = get_or_create_identity()?;

    let register_pk = RendezvousMessage {
        union: Some(RdzUnion::RegisterPk(RegisterPk {
            id: device_id.to_string(),
            uuid: uuid_bytes,
            pk: pk_bytes,
            old_id: String::new(),
            no_register_device: false,
        })),
    };

    socket
        .send(&encode_udp_msg(&register_pk))
        .await
        .context("Failed to send RegisterPk")?;
    info!("Sent RegisterPk for device {}", device_id);
    Ok(())
}

/// Handle incoming UDP messages from the server.
async fn handle_server_message_udp(
    socket: &UdpSocket,
    device_id: &str,
    msg: RendezvousMessage,
) {
    match msg.union {
        Some(RdzUnion::RegisterPeerResponse(rpr)) => {
            debug!("RegisterPeerResponse: request_pk={}", rpr.request_pk);
            if rpr.request_pk {
                if let Err(e) = send_register_pk_udp(socket, device_id).await {
                    warn!("Failed to send RegisterPk: {}", e);
                }
            }
        }
        Some(RdzUnion::RegisterPkResponse(rpkr)) => {
            debug!(
                "RegisterPkResponse: result={}, keep_alive={}",
                rpkr.result, rpkr.keep_alive
            );
        }
        Some(RdzUnion::ConfigureUpdate(cu)) => {
            debug!("ConfigUpdate received (serial={})", cu.serial);
        }
        Some(RdzUnion::PunchHole(ph)) => {
            // Someone is trying to connect to us — the server forwarded
            // the PunchHole.  We cannot do direct P2P (no UDP listener),
            // so we just log and wait for the relay flow (RelayResponse).
            info!(
                "Incoming PunchHole: relay_server={}, nat_type={}",
                ph.relay_server, ph.nat_type
            );
        }
        Some(RdzUnion::PunchHoleSent(phs)) => {
            // Variant of PunchHole with peer id and relay server.
            info!(
                "Incoming PunchHoleSent from {}: relay_server={}",
                phs.id, phs.relay_server
            );
        }
        Some(RdzUnion::RelayResponse(rr)) => {
            // The signal server forwarded a relay request — someone wants
            // to connect to us via relay.  Spawn the incoming handler.
            let uuid = rr.uuid.clone();
            let relay_server = rr.relay_server.clone();
            if uuid.is_empty() || relay_server.is_empty() {
                warn!(
                    "RelayResponse with empty uuid/relay: uuid={}, relay={}",
                    uuid, relay_server
                );
                return;
            }

            info!(
                "Incoming RelayResponse: uuid={}, relay_server={} — spawning incoming session",
                uuid, relay_server
            );

            // Read device password from config (best-effort, fallback to empty)
            let device_password = crate::config::Settings::load()
                .map(|s| s.device_password.clone())
                .unwrap_or_default();
            let my_id = device_id.to_string();

            crate::network::incoming::spawn_incoming::<tauri::Wry>(
                relay_server,
                uuid,
                device_password,
                my_id,
                None, // TODO: pass AppHandle for RemoteBadge events
            );
        }
        Some(RdzUnion::RequestRelay(rr)) => {
            // The signal server forwarded a RequestRelay to us (target).
            let uuid = rr.uuid.clone();
            let relay_server = rr.relay_server.clone();
            if uuid.is_empty() {
                warn!("RequestRelay with empty uuid");
                return;
            }

            // Determine relay server: use the one from the message, or
            // derive from our server address.
            let effective_relay = if relay_server.is_empty() {
                let settings = crate::config::Settings::load().unwrap_or_default();
                let host = settings.server_address.split(':').next().unwrap_or("").to_string();
                if host.is_empty() {
                    warn!("RequestRelay: no relay server and no server address configured");
                    return;
                }
                format!("{}:21117", host)
            } else {
                relay_server
            };

            info!(
                "Incoming RequestRelay: uuid={}, relay={} — spawning incoming session",
                uuid, effective_relay
            );

            let device_password = crate::config::Settings::load()
                .map(|s| s.device_password.clone())
                .unwrap_or_default();
            let my_id = device_id.to_string();

            crate::network::incoming::spawn_incoming::<tauri::Wry>(
                effective_relay,
                uuid,
                device_password,
                my_id,
                None, // TODO: pass AppHandle for RemoteBadge events
            );
        }
        Some(RdzUnion::FetchLocalAddr(_fla)) => {
            // Server asking for our local address — used for LAN detection.
            // Respond with our local socket address.
            debug!("FetchLocalAddr received — ignoring (relay-only mode)");
        }
        Some(other) => {
            debug!("Unhandled server message: {:?}", other);
        }
        None => {}
    }
}

/// Get or create a persistent Ed25519 keypair and UUID for RegisterPk.
///
/// Stored alongside the device ID in the config directory.
fn get_or_create_identity() -> Result<(Vec<u8>, Vec<u8>)> {
    let config_dir = directories::ProjectDirs::from("com", "yomie", "Yomie")
        .context("Failed to determine config directory")?
        .config_dir()
        .to_path_buf();

    let pk_path = config_dir.join("id_ed25519.pub");
    let sk_path = config_dir.join("id_ed25519");
    let uuid_path = config_dir.join("uuid");

    // Load or generate UUID
    let uuid_bytes = if uuid_path.exists() {
        std::fs::read(&uuid_path).context("Failed to read UUID")?
    } else {
        let uuid = uuid::Uuid::new_v4();
        std::fs::create_dir_all(&config_dir).ok();
        std::fs::write(&uuid_path, uuid.as_bytes()).context("Failed to write UUID")?;
        uuid.as_bytes().to_vec()
    };

    // Load or generate Ed25519 keypair
    let pk_bytes = if pk_path.exists() && sk_path.exists() {
        std::fs::read(&pk_path).context("Failed to read public key")?
    } else {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        std::fs::create_dir_all(&config_dir).ok();
        std::fs::write(&sk_path, signing_key.to_bytes())
            .context("Failed to write secret key")?;
        std::fs::write(&pk_path, verifying_key.to_bytes())
            .context("Failed to write public key")?;

        info!("Generated new Ed25519 keypair for device registration");
        verifying_key.to_bytes().to_vec()
    };

    Ok((pk_bytes, uuid_bytes))
}

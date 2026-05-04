//! UDP broadcast scanner for Yomie servers on the local network.
//!
//! Protocol (matches `web-nodejs/services/lanDiscovery.js`):
//!   Client → broadcast 255.255.255.255:21119
//!     { "type": "yomie-discover", "version": 1 }
//!
//!   Server → unicast reply
//!     { "type": "yomie-announce", "version": 1, "server": { ... } }

use anyhow::Result;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::sync::watch;
use tokio::time::{sleep, Duration, Instant};

// ---------------------------------------------------------------------------
//  Constants
// ---------------------------------------------------------------------------

const DISCOVERY_PORT: u16 = 21119;
const PROBE_INTERVAL: Duration = Duration::from_secs(3);
const SCAN_TIMEOUT: Duration = Duration::from_secs(30);
const RECV_BUF_SIZE: usize = 4096;

// ---------------------------------------------------------------------------
//  Types
// ---------------------------------------------------------------------------

/// Probe sent by the client.
#[derive(Debug, Serialize)]
struct DiscoverProbe {
    #[serde(rename = "type")]
    msg_type: String,
    version: u32,
}

/// Server info returned in announce response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub port: u16,
    #[serde(rename = "apiPort", default)]
    pub api_port: u16,
    #[serde(default)]
    pub protocol: String,
    #[serde(rename = "publicKey", default)]
    pub public_key: String,
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(rename = "discoveryPort", default)]
    pub discovery_port: u16,
}

/// Announce response from the server.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AnnounceResponse {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    version: u32,
    server: ServerInfo,
}

/// A discovered Yomie server with connection details.
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredServer {
    /// Server display name (hostname).
    pub name: String,
    /// Server version string.
    pub version: String,
    /// IP address (from the UDP reply source).
    pub address: String,
    /// Web console port (typically 5000).
    pub port: u16,
    /// Client API port (typically 21121).
    pub api_port: u16,
    /// Protocol (http / https).
    pub protocol: String,
    /// Server public key (base64 Ed25519).
    pub public_key: String,
    /// All known IP addresses of the server.
    pub addresses: Vec<String>,
    /// Console URL derived from address + port.
    pub console_url: String,
    /// hbbs address (host:21116).
    pub server_address: String,
    /// When this server was last seen.
    pub last_seen_ms: u64,
}

/// Overall discovery status.
#[derive(Debug, Clone, Serialize)]
pub struct LanDiscoveryStatus {
    pub scanning: bool,
    pub servers: Vec<DiscoveredServer>,
    pub scans_completed: u32,
    pub last_error: Option<String>,
}

// ---------------------------------------------------------------------------
//  Service handle
// ---------------------------------------------------------------------------

pub struct LanDiscoveryService {
    status_rx: watch::Receiver<LanDiscoveryStatus>,
    cancel_tx: watch::Sender<bool>,
}

impl LanDiscoveryService {
    /// Start LAN discovery scanning in the background.
    pub fn start() -> Self {
        let initial = LanDiscoveryStatus {
            scanning: true,
            servers: Vec::new(),
            scans_completed: 0,
            last_error: None,
        };

        let (status_tx, status_rx) = watch::channel(initial);
        let (cancel_tx, cancel_rx) = watch::channel(false);

        tauri::async_runtime::spawn(async move {
            discovery_loop(status_tx, cancel_rx).await;
        });

        LanDiscoveryService {
            status_rx,
            cancel_tx,
        }
    }

    /// Get the current discovery status including discovered servers.
    pub fn status(&self) -> LanDiscoveryStatus {
        self.status_rx.borrow().clone()
    }

    /// Get the list of discovered servers.
    pub fn discovered_servers(&self) -> Vec<DiscoveredServer> {
        self.status_rx.borrow().servers.clone()
    }

    /// Stop the discovery service.
    pub fn stop(&self) {
        let _ = self.cancel_tx.send(true);
        info!("LAN discovery service stop requested");
    }
}

// ---------------------------------------------------------------------------
//  Main loop
// ---------------------------------------------------------------------------

async fn discovery_loop(
    status_tx: watch::Sender<LanDiscoveryStatus>,
    mut cancel_rx: watch::Receiver<bool>,
) {
    let mut servers: HashMap<String, DiscoveredServer> = HashMap::new();
    let mut scans_completed: u32 = 0;
    let start = Instant::now();

    loop {
        if *cancel_rx.borrow() {
            info!("LAN discovery cancelled");
            let mut status = status_tx.borrow().clone();
            status.scanning = false;
            let _ = status_tx.send(status);
            return;
        }

        // Stop after overall timeout
        if start.elapsed() > SCAN_TIMEOUT {
            info!("LAN discovery scan timeout reached ({} scans)", scans_completed);
            let mut status = status_tx.borrow().clone();
            status.scanning = false;
            let _ = status_tx.send(status);
            return;
        }

        // Send probe and collect replies
        match send_probe_and_collect().await {
            Ok(found) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                for (addr, info) in found {
                    let server = build_discovered_server(&addr, &info, now);
                    let key = format!("{}:{}", server.address, server.port);
                    servers.insert(key, server);
                }

                scans_completed += 1;

                let _ = status_tx.send(LanDiscoveryStatus {
                    scanning: true,
                    servers: servers.values().cloned().collect(),
                    scans_completed,
                    last_error: None,
                });

                debug!("Scan #{}: {} server(s) found", scans_completed, servers.len());
            }
            Err(e) => {
                warn!("Discovery probe failed: {}", e);
                let _ = status_tx.send(LanDiscoveryStatus {
                    scanning: true,
                    servers: servers.values().cloned().collect(),
                    scans_completed,
                    last_error: Some(e.to_string()),
                });
            }
        }

        // Wait before next probe (or cancel)
        tokio::select! {
            _ = sleep(PROBE_INTERVAL) => {}
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    let mut status = status_tx.borrow().clone();
                    status.scanning = false;
                    let _ = status_tx.send(status);
                    return;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
//  UDP probe
// ---------------------------------------------------------------------------

async fn send_probe_and_collect() -> Result<Vec<(SocketAddr, ServerInfo)>> {
    // Bind to any available port on all interfaces
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket.set_broadcast(true)?;

    // Build probe message
    let probe = DiscoverProbe {
        msg_type: "yomie-discover".into(),
        version: 1,
    };
    let probe_bytes = serde_json::to_vec(&probe)?;

    // Broadcast
    let dest: SocketAddr = format!("255.255.255.255:{}", DISCOVERY_PORT).parse()?;
    socket.send_to(&probe_bytes, dest).await?;

    // Collect replies for up to 2 seconds
    let mut results = Vec::new();
    let mut buf = [0u8; RECV_BUF_SIZE];
    let deadline = Instant::now() + Duration::from_secs(2);

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
            Ok(Ok((len, addr))) => {
                if let Ok(response) = serde_json::from_slice::<AnnounceResponse>(&buf[..len]) {
                    if response.msg_type == "yomie-announce" {
                        results.push((addr, response.server));
                    }
                }
            }
            Ok(Err(e)) => {
                warn!("UDP recv error: {}", e);
                break;
            }
            Err(_) => {
                // Timeout — no more replies
                break;
            }
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

fn build_discovered_server(
    addr: &SocketAddr,
    info: &ServerInfo,
    now_ms: u64,
) -> DiscoveredServer {
    let ip = addr.ip().to_string();

    let protocol = if info.protocol.is_empty() {
        "http".to_string()
    } else {
        info.protocol.clone()
    };

    let console_port = if info.port > 0 { info.port } else { 5000 };
    let console_url = format!("{}://{}:{}", protocol, ip, console_port);
    let server_address = format!("{}:21116", ip);

    DiscoveredServer {
        name: info.name.clone(),
        version: info.version.clone(),
        address: ip,
        port: console_port,
        api_port: if info.api_port > 0 { info.api_port } else { 21121 },
        protocol,
        public_key: info.public_key.clone(),
        addresses: info.addresses.clone(),
        console_url,
        server_address,
        last_seen_ms: now_ms,
    }
}

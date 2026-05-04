//! mDNS/DNS-SD discovery for Yomie servers on the local network.
//!
//! Discovers servers advertising `_betterdesk._tcp.local.` via mDNS.
//! This complements the UDP broadcast scanner for environments where
//! broadcast is blocked but mDNS works (e.g., some Wi-Fi networks).
//!
//! The server advertises TXT records:
//!   version=<version>
//!   port=<console_port>
//!   apiPort=<api_port>
//!   protocol=<http|https>
//!   publicKey=<base64_ed25519>

use log::{debug, info, warn};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tokio::time::Duration;

const SERVICE_TYPE: &str = "_betterdesk._tcp.local.";
const BROWSE_TIMEOUT: Duration = Duration::from_secs(15);

/// A Yomie server discovered via mDNS.
#[derive(Debug, Clone, Serialize)]
pub struct MdnsServer {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub addresses: Vec<String>,
    pub version: String,
    pub api_port: u16,
    pub protocol: String,
    pub public_key: String,
    pub console_url: String,
    pub server_address: String,
}

/// Browse for Yomie servers via mDNS for `timeout` duration.
/// Returns a list of discovered servers.
pub fn browse_mdns_servers(timeout: Duration) -> Vec<MdnsServer> {
    let servers: Arc<Mutex<HashMap<String, MdnsServer>>> = Arc::new(Mutex::new(HashMap::new()));

    let daemon = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => {
            warn!("mDNS: failed to create daemon: {}", e);
            return Vec::new();
        }
    };

    let receiver = match daemon.browse(SERVICE_TYPE) {
        Ok(r) => r,
        Err(e) => {
            warn!("mDNS: failed to start browse: {}", e);
            return Vec::new();
        }
    };

    info!("mDNS: browsing for {} (timeout: {:?})", SERVICE_TYPE, timeout);

    let deadline = std::time::Instant::now() + timeout;

    while std::time::Instant::now() < deadline {
        match receiver.recv_timeout(std::time::Duration::from_millis(500)) {
            Ok(ServiceEvent::ServiceResolved(info)) => {
                debug!("mDNS: resolved service: {} at {}:{}", info.get_fullname(), info.get_hostname(), info.get_port());

                let txt = info.get_properties();
                let version = txt.get("version").map(|v| v.val_str().to_string()).unwrap_or_default();
                let api_port: u16 = txt.get("apiPort")
                    .and_then(|v| v.val_str().parse().ok())
                    .unwrap_or(21114);
                let protocol = txt.get("protocol").map(|v| v.val_str().to_string()).unwrap_or_else(|| "http".into());
                let public_key = txt.get("publicKey").map(|v| v.val_str().to_string()).unwrap_or_default();

                let addresses: Vec<String> = info.get_addresses()
                    .iter()
                    .map(|a| a.to_string())
                    .collect();

                let host = addresses.first()
                    .cloned()
                    .unwrap_or_else(|| info.get_hostname().trim_end_matches('.').to_string());

                let console_url = format!("{}://{}:{}", protocol, host, info.get_port());
                let server_address = format!("{}:21116", host);

                let server = MdnsServer {
                    name: info.get_fullname().split('.').next().unwrap_or("Yomie").to_string(),
                    host: host.clone(),
                    port: info.get_port(),
                    addresses,
                    version,
                    api_port,
                    protocol,
                    public_key,
                    console_url,
                    server_address,
                };

                if let Ok(mut map) = servers.lock() {
                    map.insert(server.name.clone(), server);
                }
            }
            Ok(ServiceEvent::ServiceRemoved(_, name)) => {
                debug!("mDNS: service removed: {}", name);
                if let Ok(mut map) = servers.lock() {
                    map.remove(&name);
                }
            }
            Ok(_) => {} // SearchStarted, etc.
            Err(_) => {} // timeout, continue
        }
    }

    let _ = daemon.stop_browse(SERVICE_TYPE);
    let _ = daemon.shutdown();

    let result: Vec<MdnsServer> = servers.lock()
        .map(|m| m.values().cloned().collect())
        .unwrap_or_default();

    info!("mDNS: browse complete, found {} server(s)", result.len());
    result
}

/// Start a background mDNS browse task. Results pushed to the provided watch channel.
pub fn spawn_mdns_browse(
    results_tx: watch::Sender<Vec<MdnsServer>>,
    cancel_rx: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::task::spawn_blocking(move || {
        loop {
            if *cancel_rx.borrow() {
                info!("mDNS: browse cancelled");
                return;
            }

            let servers = browse_mdns_servers(BROWSE_TIMEOUT);
            let _ = results_tx.send(servers);

            // Wait before next scan (or exit if cancelled)
            std::thread::sleep(std::time::Duration::from_secs(30));
            if *cancel_rx.borrow() {
                return;
            }
        }
    })
}

//! LAN Discovery — automatic Yomie server detection.
//!
//! Sends UDP broadcast probes on port 21119. Servers running the Yomie
//! LAN Discovery service reply with their identity (name, version, address,
//! public key, etc.).
//!
//! Usage from Tauri commands:
//! ```
//! let service = LanDiscoveryService::start();
//! let servers = service.discovered_servers();
//! service.stop();
//! ```

pub mod scanner;
pub mod mdns;

pub use scanner::{DiscoveredServer, LanDiscoveryService, LanDiscoveryStatus};
pub use mdns::{MdnsServer, browse_mdns_servers};

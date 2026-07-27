//! Network layer — signal server client, relay client, peer session, and registration.
//!
//! Legacy RustDesk modules (registration, signal, relay) are kept for
//! compatibility.  New codenextremote-native modules (bd_registration, bd_relay)
//! use HTTP + WebSocket instead.

pub mod registration;
pub mod relay;
pub mod session;
pub mod signal;
pub mod incoming;

// codenextremote native protocol modules
pub mod bd_registration;
pub mod bd_relay;

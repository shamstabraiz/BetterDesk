//! Device identity — persistent 9-digit numeric ID.
//!
//! ID generation:
//! 1. Derive from machine-specific hardware fingerprint (MAC + hostname + OS).
//! 2. Fall back to random generation if fingerprint is unavailable.
//! 3. Persist to `<config_dir>/id` file so the ID survives restarts.

mod device;

pub use device::get_or_create_device_id;

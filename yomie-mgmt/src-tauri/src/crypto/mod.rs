//! Cryptographic primitives — NaCl-compatible encryption.
//!
//! Provides:
//! - Ephemeral Curve25519 key exchange (NaCl box)
//! - Counter-based secretbox stream encryption (XSalsa20-Poly1305)
//! - Ed25519 signature verification
//! - Password hashing for RustDesk authentication
//! - Yomie native E2E encryption (X25519 + XSalsa20-Poly1305)

mod exchange;
mod keys;
mod secretbox;
pub mod bd_e2e;

pub use exchange::{KeyExchange, hash_password};
pub use keys::{EphemeralKeyPair, verify_signed_id, SignedIdPayload};
pub use secretbox::SecretBoxStream;
pub use bd_e2e::BdE2E;

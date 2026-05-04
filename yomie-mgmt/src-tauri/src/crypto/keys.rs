//! Ephemeral key pair generation and SignedId verification.

use anyhow::{bail, Context, Result};
use crypto_box::aead::OsRng;
use x25519_dalek::{PublicKey, StaticSecret};

/// An ephemeral Curve25519 key pair for NaCl box key exchange.
pub struct EphemeralKeyPair {
    /// Our secret key (never leaves memory).
    pub secret: StaticSecret,
    /// Our public key (sent to the peer).
    pub public: PublicKey,
}

impl EphemeralKeyPair {
    /// Generate a fresh ephemeral Curve25519 key pair.
    pub fn generate() -> Self {
        let secret = StaticSecret::random_from_rng(OsRng);
        let public = PublicKey::from(&secret);
        Self { secret, public }
    }
}

/// Parsed payload from a RustDesk SignedId message.
pub struct SignedIdPayload {
    /// Peer's device ID string.
    pub peer_id: String,
    /// Peer's ephemeral Curve25519 public key (32 bytes).
    pub peer_pk: [u8; 32],
    /// Ed25519 signature over the IdPk protobuf payload (64 bytes).
    pub signature: [u8; 64],
    /// Raw IdPk protobuf bytes (for signature verification).
    pub payload: Vec<u8>,
}

/// Parse a SignedId.id field into its components.
///
/// Format: `[64-byte Ed25519 signature][protobuf(IdPk { id, pk })]`
///
/// The `pk` field in IdPk is the peer's ephemeral Curve25519 public key.
/// We do NOT verify the Ed25519 signature here because we may not have the
/// peer's long-term Ed25519 public key.  The signature is preserved for
/// optional verification later.
pub fn verify_signed_id(signed_id_bytes: &[u8]) -> Result<SignedIdPayload> {
    if signed_id_bytes.len() < 64 + 4 {
        bail!(
            "SignedId too short: {} bytes (need >= 68)",
            signed_id_bytes.len()
        );
    }

    let mut signature = [0u8; 64];
    signature.copy_from_slice(&signed_id_bytes[..64]);
    let payload = signed_id_bytes[64..].to_vec();

    // Decode IdPk protobuf: { string id = 1; bytes pk = 2; }
    let id_pk = crate::proto::IdPk::decode(&*payload)
        .context("Failed to decode IdPk protobuf")?;

    let peer_id = id_pk.id;
    let pk_bytes = id_pk.pk;
    if pk_bytes.len() != 32 {
        bail!("Invalid peer pk length: {} (expected 32)", pk_bytes.len());
    }

    let mut peer_pk = [0u8; 32];
    peer_pk.copy_from_slice(&pk_bytes);

    Ok(SignedIdPayload {
        peer_id,
        peer_pk,
        signature,
        payload,
    })
}

use prost::Message;

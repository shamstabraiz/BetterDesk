//! NaCl box key exchange — Curve25519-XSalsa20-Poly1305.
//!
//! Matches RustDesk's key exchange protocol:
//! 1. Both sides generate ephemeral Curve25519 keypairs.
//! 2. The initiator generates a random 32-byte symmetric key.
//! 3. The initiator encrypts the symmetric key using NaCl box
//!    (Curve25519 DH + XSalsa20-Poly1305) with a zero nonce.
//! 4. The initiator sends PublicKey { our_pk, sealed_key } to the target.
//! 5. The target opens the box to recover the symmetric key.
//! 6. Both sides use the symmetric key for secretbox stream encryption.

use anyhow::{bail, Result};
use crypto_box::aead::{Aead, OsRng};
use crypto_box::{PublicKey, SalsaBox, SecretKey};
use rand::RngCore;

use super::keys::EphemeralKeyPair;
use super::secretbox::SecretBoxStream;

/// Manages the key exchange state machine.
pub struct KeyExchange {
    /// Our ephemeral keypair.
    keypair: EphemeralKeyPair,
    /// Generated symmetric key (32 bytes).
    symmetric_key: [u8; 32],
}

impl KeyExchange {
    /// Create a new key exchange (initiator role).
    /// Generates an ephemeral keypair and a random symmetric key.
    pub fn new_initiator() -> Self {
        let keypair = EphemeralKeyPair::generate();
        let mut symmetric_key = [0u8; 32];
        OsRng.fill_bytes(&mut symmetric_key);

        Self {
            keypair,
            symmetric_key,
        }
    }

    /// Create the PublicKey message payload for sending to the peer.
    ///
    /// Returns `(asymmetric_value, symmetric_value)`:
    /// - `asymmetric_value` = our ephemeral Curve25519 pk (32 bytes)
    /// - `symmetric_value` = NaCl box encrypted symmetric key (48 bytes)
    pub fn create_public_key_msg(&self, their_pk_bytes: &[u8; 32]) -> Result<(Vec<u8>, Vec<u8>)> {
        // Convert raw bytes to crypto_box types
        let our_sk = SecretKey::from(self.keypair.secret.to_bytes());
        let their_pk = PublicKey::from(*their_pk_bytes);

        // Create NaCl box (DH + XSalsa20-Poly1305)
        let salsa_box = SalsaBox::new(&their_pk, &our_sk);

        // Encrypt with zero nonce (safe because ephemeral keys are one-time)
        let zero_nonce = crypto_box::Nonce::default(); // 24 bytes of zeros
        let sealed = salsa_box
            .encrypt(&zero_nonce, self.symmetric_key.as_ref())
            .map_err(|e| anyhow::anyhow!("NaCl box encrypt failed: {}", e))?;

        Ok((
            self.keypair.public.as_bytes().to_vec(), // 32 bytes
            sealed,                                    // 48 bytes (16 MAC + 32 key)
        ))
    }

    /// Open a received PublicKey message (responder role).
    ///
    /// Extracts the symmetric key from the NaCl box.
    /// Used when we are the target and the initiator sent their PublicKey.
    pub fn open_public_key(
        our_keypair: &EphemeralKeyPair,
        their_pk_bytes: &[u8; 32],
        sealed_key: &[u8],
    ) -> Result<[u8; 32]> {
        if sealed_key.len() != 48 {
            bail!(
                "Invalid sealed key length: {} (expected 48)",
                sealed_key.len()
            );
        }

        let our_sk = SecretKey::from(our_keypair.secret.to_bytes());
        let their_pk = PublicKey::from(*their_pk_bytes);

        let salsa_box = SalsaBox::new(&their_pk, &our_sk);
        let zero_nonce = crypto_box::Nonce::default();

        let plaintext = salsa_box
            .decrypt(&zero_nonce, sealed_key)
            .map_err(|e| anyhow::anyhow!("NaCl box decrypt failed: {}", e))?;

        if plaintext.len() != 32 {
            bail!(
                "Decrypted key wrong length: {} (expected 32)",
                plaintext.len()
            );
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&plaintext);
        Ok(key)
    }

    /// Finalize the key exchange and produce a SecretBoxStream.
    pub fn finalize(self) -> SecretBoxStream {
        SecretBoxStream::new(&self.symmetric_key)
    }

    /// Get the symmetric key bytes (for creating the stream separately).
    pub fn symmetric_key(&self) -> &[u8; 32] {
        &self.symmetric_key
    }

    /// Get our ephemeral public key bytes.
    pub fn our_pk(&self) -> &[u8; 32] {
        self.keypair.public.as_bytes()
    }
}

/// Hash a password for RustDesk login authentication.
///
/// Algorithm (matches RustDesk):
///   `intermediate = SHA-256(password || salt)`
///   `final        = SHA-256(intermediate || challenge)`
pub fn hash_password(password: &str, salt: &str, challenge: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};

    // Step 1: intermediate = sha256(password + salt)
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(salt.as_bytes());
    let intermediate = hasher.finalize();

    // Step 2: final = sha256(intermediate + challenge)
    let mut hasher = Sha256::new();
    hasher.update(intermediate);
    hasher.update(challenge.as_bytes());
    let result = hasher.finalize();

    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_exchange_round_trip() {
        // Simulate initiator-target key exchange
        let initiator = KeyExchange::new_initiator();
        let target_kp = EphemeralKeyPair::generate();

        // Initiator creates PublicKey message
        let (asym, sym) = initiator
            .create_public_key_msg(target_kp.public.as_bytes())
            .unwrap();

        // Target opens the box to get the symmetric key
        let mut their_pk = [0u8; 32];
        their_pk.copy_from_slice(&asym);
        let recovered_key =
            KeyExchange::open_public_key(&target_kp, &their_pk, &sym).unwrap();

        // Both sides should have the same key
        assert_eq!(recovered_key, *initiator.symmetric_key());
    }

    #[test]
    fn password_hash_deterministic() {
        let h1 = hash_password("test", "salt1", "challenge1");
        let h2 = hash_password("test", "salt1", "challenge1");
        assert_eq!(h1, h2);

        let h3 = hash_password("test", "salt1", "challenge2");
        assert_ne!(h1, h3);
    }
}

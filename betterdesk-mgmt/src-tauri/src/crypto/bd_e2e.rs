//! Yomie native E2E encryption for relay sessions.
//!
//! Uses X25519 Diffie-Hellman + XSalsa20-Poly1305 to establish a shared
//! symmetric key between two Yomie desktop clients.  The server
//! never sees the plaintext.
//!
//! Protocol (over the relay WebSocket):
//!   1. Both sides generate ephemeral X25519 keypairs
//!   2. Initiator sends: { "type": "key_exchange", "pk": base64(32 bytes) }
//!   3. Target sends:    { "type": "key_exchange", "pk": base64(32 bytes) }
//!   4. Both compute: shared_secret = X25519(our_sk, their_pk)
//!   5. Derive key:   symmetric_key = SHA256(shared_secret ‖ "Yomie-E2E-v1")
//!   6. All subsequent binary frames: nonce(24) ‖ ciphertext(N+16)
//!
//! The relay server sees only opaque binary blobs.

use anyhow::{bail, Context, Result};
use crypto_box::aead::OsRng;
use log::info;
use sha2::{Digest, Sha256};
use x25519_dalek::{PublicKey, StaticSecret};
use xsalsa20poly1305::{
    aead::{Aead, KeyInit},
    Nonce, XSalsa20Poly1305,
};

const DOMAIN_SEPARATOR: &[u8] = b"Yomie-E2E-v1";

/// E2E encryption context for a relay session.
pub struct BdE2E {
    /// Our ephemeral secret key.
    our_sk: StaticSecret,
    /// Our ephemeral public key (base64-encoded for JSON exchange).
    our_pk: PublicKey,
    /// Derived symmetric cipher (set after key exchange).
    cipher: Option<XSalsa20Poly1305>,
    /// Incrementing nonce counter (prevents nonce reuse).
    send_counter: u64,
}

impl BdE2E {
    /// Create a new E2E context (generates ephemeral keypair).
    pub fn new() -> Self {
        let sk = StaticSecret::random_from_rng(OsRng);
        let pk = PublicKey::from(&sk);
        BdE2E {
            our_sk: sk,
            our_pk: pk,
            cipher: None,
            send_counter: 0,
        }
    }

    /// Get our public key as base64 (for the key_exchange JSON message).
    pub fn public_key_base64(&self) -> String {
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            self.our_pk.as_bytes(),
        )
    }

    /// Complete the key exchange given the peer's public key.
    pub fn complete_exchange(&mut self, their_pk_base64: &str) -> Result<()> {
        let their_pk_bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            their_pk_base64,
        )
        .context("Invalid base64 for peer public key")?;

        if their_pk_bytes.len() != 32 {
            bail!("Invalid peer PK length: {}", their_pk_bytes.len());
        }

        let mut pk_array = [0u8; 32];
        pk_array.copy_from_slice(&their_pk_bytes);
        let their_pk = PublicKey::from(pk_array);

        // X25519 Diffie-Hellman
        let shared_secret = self.our_sk.diffie_hellman(&their_pk);

        // Derive symmetric key: SHA256(shared_secret || domain_separator)
        let mut hasher = Sha256::new();
        hasher.update(shared_secret.as_bytes());
        hasher.update(DOMAIN_SEPARATOR);
        let symmetric_key: [u8; 32] = hasher.finalize().into();

        self.cipher = Some(XSalsa20Poly1305::new(&symmetric_key.into()));
        self.send_counter = 0;

        info!("E2E key exchange completed");
        Ok(())
    }

    /// Encrypt a plaintext frame for sending.
    ///
    /// Returns: nonce(24 bytes) || ciphertext(len + 16 bytes MAC)
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = self
            .cipher
            .as_ref()
            .context("E2E not initialized — key exchange not completed")?;

        // Build nonce from counter (24 bytes, little-endian counter in first 8)
        let mut nonce_bytes = [0u8; 24];
        nonce_bytes[..8].copy_from_slice(&self.send_counter.to_le_bytes());
        self.send_counter += 1;

        let nonce = Nonce::from(nonce_bytes);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        // Prepend nonce
        let mut output = Vec::with_capacity(24 + ciphertext.len());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);

        Ok(output)
    }

    /// Decrypt a received frame.
    ///
    /// Expects: nonce(24 bytes) || ciphertext(N + 16 bytes MAC)
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let cipher = self
            .cipher
            .as_ref()
            .context("E2E not initialized — key exchange not completed")?;

        if data.len() < 24 + 16 {
            bail!("Encrypted frame too short: {} bytes", data.len());
        }

        let nonce = Nonce::from_slice(&data[..24]);
        let ciphertext = &data[24..];

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| anyhow::anyhow!("Decryption failed — invalid key or corrupted data"))
    }

    /// Check if key exchange is complete and encryption is ready.
    pub fn is_ready(&self) -> bool {
        self.cipher.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_exchange_and_encrypt_decrypt() {
        let mut alice = BdE2E::new();
        let mut bob = BdE2E::new();

        // Exchange public keys
        let alice_pk = alice.public_key_base64();
        let bob_pk = bob.public_key_base64();

        alice.complete_exchange(&bob_pk).unwrap();
        bob.complete_exchange(&alice_pk).unwrap();

        assert!(alice.is_ready());
        assert!(bob.is_ready());

        // Alice encrypts, Bob decrypts
        let plaintext = b"Hello from Yomie!";
        let encrypted = alice.encrypt(plaintext).unwrap();
        let decrypted = bob.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);

        // Bob encrypts, Alice decrypts
        let plaintext2 = b"Response from Bob";
        let encrypted2 = bob.encrypt(plaintext2).unwrap();
        let decrypted2 = alice.decrypt(&encrypted2).unwrap();
        assert_eq!(decrypted2, plaintext2);
    }

    #[test]
    fn test_tampered_data_fails() {
        let mut alice = BdE2E::new();
        let mut bob = BdE2E::new();

        alice.complete_exchange(&bob.public_key_base64()).unwrap();
        bob.complete_exchange(&alice.public_key_base64()).unwrap();

        let encrypted = alice.encrypt(b"secret data").unwrap();

        // Tamper with ciphertext
        let mut tampered = encrypted.clone();
        if let Some(last) = tampered.last_mut() {
            *last ^= 0xff;
        }

        assert!(bob.decrypt(&tampered).is_err());
    }
}

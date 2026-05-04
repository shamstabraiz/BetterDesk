//! NaCl secretbox stream encryption with counter-based nonces.
//!
//! Matches RustDesk's `FramedStream` encryption:
//! - Key: 32-byte XSalsa20-Poly1305 symmetric key
//! - Nonce: u64 counter in LE bytes, zero-padded to 24 bytes
//! - Output: 16-byte Poly1305 MAC + ciphertext (no nonce prefix)
//! - Counter is pre-incremented: first message uses nonce=1

use anyhow::Result;
use xsalsa20poly1305::aead::{Aead, KeyInit};
use xsalsa20poly1305::{Key, Nonce, XSalsa20Poly1305};

/// Bidirectional stream cipher with counter-based nonces.
pub struct SecretBoxStream {
    cipher: XSalsa20Poly1305,
    send_seq: u64,
    recv_seq: u64,
}

impl SecretBoxStream {
    /// Create a new stream cipher with the given 32-byte symmetric key.
    pub fn new(key: &[u8; 32]) -> Self {
        let cipher = XSalsa20Poly1305::new(Key::from_slice(key));
        Self {
            cipher,
            send_seq: 0,
            recv_seq: 0,
        }
    }

    /// Encrypt data.  Pre-increments the send counter.
    /// Returns MAC (16 bytes) + ciphertext.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        self.send_seq += 1;
        let nonce = Self::counter_nonce(self.send_seq);
        self.cipher
            .encrypt(&nonce, plaintext)
            .map_err(|e| anyhow::anyhow!("Encrypt failed (seq={}): {}", self.send_seq, e))
    }

    /// Decrypt data.  Pre-increments the receive counter.
    /// Input: MAC (16 bytes) + ciphertext.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        self.recv_seq += 1;
        let nonce = Self::counter_nonce(self.recv_seq);
        self.cipher
            .decrypt(&nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decrypt failed (seq={}): {}", self.recv_seq, e))
    }

    /// Try to decrypt WITHOUT advancing the receive counter.
    /// Returns the plaintext and the would-be next sequence number on success.
    pub fn try_decrypt(&self, ciphertext: &[u8]) -> Option<(Vec<u8>, u64)> {
        let next_seq = self.recv_seq + 1;
        let nonce = Self::counter_nonce(next_seq);
        self.cipher
            .decrypt(&nonce, ciphertext)
            .ok()
            .map(|plaintext| (plaintext, next_seq))
    }

    /// Commit the receive counter after a successful [`try_decrypt`].
    pub fn commit_recv(&mut self, seq: u64) {
        self.recv_seq = seq;
    }

    /// Current send sequence (for diagnostics).
    pub fn send_seq(&self) -> u64 {
        self.send_seq
    }

    /// Current recv sequence (for diagnostics).
    pub fn recv_seq(&self) -> u64 {
        self.recv_seq
    }

    /// Build a 24-byte nonce from a u64 counter (little-endian, zero-padded).
    fn counter_nonce(counter: u64) -> Nonce {
        let mut nonce_bytes = [0u8; 24];
        nonce_bytes[..8].copy_from_slice(&counter.to_le_bytes());
        *Nonce::from_slice(&nonce_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trip() {
        let key = [42u8; 32];
        let mut alice = SecretBoxStream::new(&key);
        let mut bob = SecretBoxStream::new(&key);

        let plaintext = b"Hello, Yomie!";
        let ciphertext = alice.encrypt(plaintext).unwrap();
        let decrypted = bob.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn counter_increments() {
        let key = [7u8; 32];
        let mut stream = SecretBoxStream::new(&key);
        assert_eq!(stream.send_seq(), 0);
        let _ = stream.encrypt(b"msg1").unwrap();
        assert_eq!(stream.send_seq(), 1);
        let _ = stream.encrypt(b"msg2").unwrap();
        assert_eq!(stream.send_seq(), 2);
    }

    #[test]
    fn try_decrypt_does_not_advance() {
        let key = [99u8; 32];
        let mut alice = SecretBoxStream::new(&key);
        let bob = SecretBoxStream::new(&key);

        let ct = alice.encrypt(b"secret").unwrap();
        let result = bob.try_decrypt(&ct);
        assert!(result.is_some());
        assert_eq!(bob.recv_seq(), 0); // unchanged
    }

    #[test]
    fn wrong_key_fails() {
        let mut alice = SecretBoxStream::new(&[1u8; 32]);
        let mut bob = SecretBoxStream::new(&[2u8; 32]);

        let ct = alice.encrypt(b"test").unwrap();
        assert!(bob.decrypt(&ct).is_err());
    }
}

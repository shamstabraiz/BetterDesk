// Package cdap — crypto provides X25519 key exchange and XSalsa20-Poly1305
// authenticated encryption for end-to-end encrypted media channels between
// the browser (viewer) and the CDAP device. The server sees only opaque
// ciphertext — it cannot decrypt the media frames.
package cdap

import (
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"sync"
	"time"

	"golang.org/x/crypto/curve25519"
	"golang.org/x/crypto/nacl/box"
)

// MediaCrypto holds the E2E key material for a single media session.
// The session key is derived via X25519 Diffie-Hellman between the viewer
// and the device. The server only facilitates the key exchange messages.
type MediaCrypto struct {
	mu           sync.Mutex
	sessionID    string
	localPub     [32]byte
	localPriv    [32]byte
	remotePub    [32]byte
	sharedSecret [32]byte
	ready        bool
	created      time.Time
}

// KeyExchangePayload is sent between viewer and device via control messages.
type KeyExchangePayload struct {
	Type      string `json:"type"`       // "key_exchange"
	SessionID string `json:"session_id"` // media session ID
	PublicKey string `json:"public_key"` // base64-encoded X25519 public key
}

// EncryptedFrame wraps a media frame with authenticated encryption.
type EncryptedFrame struct {
	Type       string `json:"type"`       // "encrypted_frame"
	SessionID  string `json:"session_id"` // media session ID
	Nonce      string `json:"nonce"`      // base64-encoded 24-byte nonce
	Ciphertext string `json:"ciphertext"` // base64-encoded XSalsa20-Poly1305 ciphertext
}

// NewMediaCrypto generates a new X25519 keypair for a media session.
func NewMediaCrypto(sessionID string) (*MediaCrypto, error) {
	pub, priv, err := box.GenerateKey(rand.Reader)
	if err != nil {
		return nil, fmt.Errorf("generate X25519 keypair: %w", err)
	}
	return &MediaCrypto{
		sessionID: sessionID,
		localPub:  *pub,
		localPriv: *priv,
		created:   time.Now(),
	}, nil
}

// PublicKeyBase64 returns the local public key as base64 for key exchange.
func (mc *MediaCrypto) PublicKeyBase64() string {
	return base64.StdEncoding.EncodeToString(mc.localPub[:])
}

// CompleteExchange derives the shared secret from the remote public key.
func (mc *MediaCrypto) CompleteExchange(remotePubB64 string) error {
	mc.mu.Lock()
	defer mc.mu.Unlock()

	pubBytes, err := base64.StdEncoding.DecodeString(remotePubB64)
	if err != nil {
		return fmt.Errorf("decode remote public key: %w", err)
	}
	if len(pubBytes) != 32 {
		return errors.New("invalid X25519 public key length")
	}

	copy(mc.remotePub[:], pubBytes)

	// Derive shared secret via X25519
	shared, err := curve25519.X25519(mc.localPriv[:], mc.remotePub[:])
	if err != nil {
		return fmt.Errorf("X25519 key exchange: %w", err)
	}
	copy(mc.sharedSecret[:], shared)

	mc.ready = true
	return nil
}

// IsReady returns true after key exchange is complete.
func (mc *MediaCrypto) IsReady() bool {
	mc.mu.Lock()
	defer mc.mu.Unlock()
	return mc.ready
}

// Encrypt encrypts plaintext using XSalsa20-Poly1305 with the shared secret.
// Returns the nonce and ciphertext.
func (mc *MediaCrypto) Encrypt(plaintext []byte) (nonce [24]byte, ciphertext []byte, err error) {
	mc.mu.Lock()
	defer mc.mu.Unlock()

	if !mc.ready {
		return nonce, nil, errors.New("key exchange not complete")
	}

	// Generate random nonce
	if _, err = rand.Read(nonce[:]); err != nil {
		return nonce, nil, fmt.Errorf("generate nonce: %w", err)
	}

	// Encrypt with NaCl box.SealAfterPrecomputation
	var sharedKey [32]byte
	box.Precompute(&sharedKey, &mc.remotePub, &mc.localPriv)
	ciphertext = box.SealAfterPrecomputation(nil, plaintext, &nonce, &sharedKey)

	return nonce, ciphertext, nil
}

// Decrypt decrypts ciphertext using XSalsa20-Poly1305 with the shared secret.
func (mc *MediaCrypto) Decrypt(nonce [24]byte, ciphertext []byte) ([]byte, error) {
	mc.mu.Lock()
	defer mc.mu.Unlock()

	if !mc.ready {
		return nil, errors.New("key exchange not complete")
	}

	var sharedKey [32]byte
	box.Precompute(&sharedKey, &mc.remotePub, &mc.localPriv)

	plaintext, ok := box.OpenAfterPrecomputation(nil, ciphertext, &nonce, &sharedKey)
	if !ok {
		return nil, errors.New("decryption failed: authentication error")
	}

	return plaintext, nil
}

// MarshalKeyExchange creates a JSON key exchange message.
func (mc *MediaCrypto) MarshalKeyExchange() ([]byte, error) {
	payload := KeyExchangePayload{
		Type:      "key_exchange",
		SessionID: mc.sessionID,
		PublicKey: mc.PublicKeyBase64(),
	}
	return json.Marshal(payload)
}

// MarshalEncryptedFrame creates a JSON encrypted frame message.
func (mc *MediaCrypto) MarshalEncryptedFrame(plaintext []byte) ([]byte, error) {
	nonce, ciphertext, err := mc.Encrypt(plaintext)
	if err != nil {
		return nil, err
	}
	frame := EncryptedFrame{
		Type:       "encrypted_frame",
		SessionID:  mc.sessionID,
		Nonce:      base64.StdEncoding.EncodeToString(nonce[:]),
		Ciphertext: base64.StdEncoding.EncodeToString(ciphertext),
	}
	return json.Marshal(frame)
}

// Zero wipes the private key and shared secret from memory.
func (mc *MediaCrypto) Zero() {
	mc.mu.Lock()
	defer mc.mu.Unlock()
	for i := range mc.localPriv {
		mc.localPriv[i] = 0
	}
	for i := range mc.sharedSecret {
		mc.sharedSecret[i] = 0
	}
	mc.ready = false
}

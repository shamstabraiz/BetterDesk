package crypto

import (
	"crypto/ed25519"
	"encoding/base64"
	"os"
	"path/filepath"
	"testing"

	pb "github.com/unitronix/betterdesk-server/proto"
	"google.golang.org/protobuf/proto"
)

func TestGenerateKeyPair(t *testing.T) {
	kp, err := GenerateKeyPair()
	if err != nil {
		t.Fatalf("GenerateKeyPair error: %v", err)
	}

	if len(kp.PrivateKey) != ed25519.PrivateKeySize {
		t.Errorf("private key size: got %d, want %d", len(kp.PrivateKey), ed25519.PrivateKeySize)
	}
	if len(kp.PublicKey) != ed25519.PublicKeySize {
		t.Errorf("public key size: got %d, want %d", len(kp.PublicKey), ed25519.PublicKeySize)
	}
}

func TestPublicKeyBase64(t *testing.T) {
	kp, err := GenerateKeyPair()
	if err != nil {
		t.Fatalf("GenerateKeyPair error: %v", err)
	}

	b64 := kp.PublicKeyBase64()
	decoded, err := base64.StdEncoding.DecodeString(b64)
	if err != nil {
		t.Fatalf("invalid base64: %v", err)
	}
	if len(decoded) != ed25519.PublicKeySize {
		t.Errorf("decoded size: got %d, want %d", len(decoded), ed25519.PublicKeySize)
	}
}

func TestSignIdPk(t *testing.T) {
	kp, err := GenerateKeyPair()
	if err != nil {
		t.Fatalf("GenerateKeyPair error: %v", err)
	}

	peerPk := make([]byte, 32)
	peerPk[0] = 0xAB // some arbitrary peer public key

	// SignIdPk returns NaCl combined format: [64-byte sig][IdPk protobuf]
	signed, err := kp.SignIdPk("TESTPEER123", peerPk)
	if err != nil {
		t.Fatalf("SignIdPk error: %v", err)
	}

	// Must be longer than just a signature (64 bytes sig + protobuf payload)
	if len(signed) <= ed25519.SignatureSize {
		t.Errorf("signed result too short: got %d, want > %d", len(signed), ed25519.SignatureSize)
	}

	// Extract signature and payload from NaCl combined format
	sig := signed[:ed25519.SignatureSize]
	payload := signed[ed25519.SignatureSize:]

	// Verify signature over the payload
	if !ed25519.Verify(kp.PublicKey, payload, sig) {
		t.Error("signature verification failed")
	}

	// Verify payload is valid IdPk protobuf
	var idpk pb.IdPk
	if err := proto.Unmarshal(payload, &idpk); err != nil {
		t.Fatalf("payload unmarshal error: %v", err)
	}
	if idpk.Id != "TESTPEER123" {
		t.Errorf("IdPk.Id: got %q, want %q", idpk.Id, "TESTPEER123")
	}
	if len(idpk.Pk) != 32 || idpk.Pk[0] != 0xAB {
		t.Errorf("IdPk.Pk mismatch: got %x", idpk.Pk)
	}
}

func TestSignIdPkDifferentDataFails(t *testing.T) {
	kp, err := GenerateKeyPair()
	if err != nil {
		t.Fatalf("GenerateKeyPair error: %v", err)
	}

	signed, err := kp.SignIdPk("PEER_A", []byte{1, 2, 3})
	if err != nil {
		t.Fatalf("SignIdPk error: %v", err)
	}

	// Extract signature from combined format
	sig := signed[:ed25519.SignatureSize]

	// Verify with different data should fail
	idpk := &pb.IdPk{Id: "PEER_B", Pk: []byte{1, 2, 3}}
	data, _ := proto.Marshal(idpk)
	if ed25519.Verify(kp.PublicKey, data, sig) {
		t.Error("signature should not verify with different data")
	}
}

func TestLoadOrGenerateKeyPair(t *testing.T) {
	dir := t.TempDir()
	basePath := filepath.Join(dir, "id_ed25519")

	// First call — should generate
	kp1, err := LoadOrGenerateKeyPair(basePath)
	if err != nil {
		t.Fatalf("first LoadOrGenerateKeyPair error: %v", err)
	}

	// Files should exist
	if _, err := os.Stat(basePath); os.IsNotExist(err) {
		t.Error("private key file not created")
	}
	if _, err := os.Stat(basePath + ".pub"); os.IsNotExist(err) {
		t.Error("public key file not created")
	}

	// Second call — should load the same key
	kp2, err := LoadOrGenerateKeyPair(basePath)
	if err != nil {
		t.Fatalf("second LoadOrGenerateKeyPair error: %v", err)
	}

	if !kp1.PublicKey.Equal(kp2.PublicKey) {
		t.Error("loaded key does not match generated key")
	}
	if kp1.PublicKeyBase64() != kp2.PublicKeyBase64() {
		t.Error("base64 public key mismatch after reload")
	}
}

func TestLoadKeyPairFromBase64(t *testing.T) {
	kp1, err := GenerateKeyPair()
	if err != nil {
		t.Fatalf("GenerateKeyPair error: %v", err)
	}

	b64 := kp1.PrivateKeyBase64()
	kp2, err := LoadKeyPairFromBase64(b64)
	if err != nil {
		t.Fatalf("LoadKeyPairFromBase64 error: %v", err)
	}

	if !kp1.PublicKey.Equal(kp2.PublicKey) {
		t.Error("keys don't match after base64 roundtrip")
	}
}

func TestLoadKeyPairFromBase64Invalid(t *testing.T) {
	// Invalid base64
	_, err := LoadKeyPairFromBase64("not-valid-base64!!!")
	if err == nil {
		t.Error("expected error for invalid base64")
	}

	// Valid base64 but wrong size
	_, err = LoadKeyPairFromBase64(base64.StdEncoding.EncodeToString([]byte("too short")))
	if err == nil {
		t.Error("expected error for wrong key size")
	}
}

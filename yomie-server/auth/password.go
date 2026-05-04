// Package auth provides authentication primitives for the Yomie server:
// password hashing (PBKDF2-HMAC-SHA256), JWT tokens (HS256), and TOTP 2FA (RFC 6238).
package auth

import (
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"fmt"
	"strings"
)

const (
	// PBKDF2-like iterations for password hashing.
	pbkdfIterations = 100_000
	saltLength      = 16
)

// HashPassword creates a salted PBKDF2-HMAC-SHA256 hash of the password.
// Returns "hex(salt):hex(hash)".
func HashPassword(password string) (string, error) {
	salt := make([]byte, saltLength)
	if _, err := rand.Read(salt); err != nil {
		return "", fmt.Errorf("auth: generate salt: %w", err)
	}
	hash := deriveKey(password, salt, pbkdfIterations)
	return fmt.Sprintf("%s:%s", hex.EncodeToString(salt), hex.EncodeToString(hash)), nil
}

// VerifyPassword checks a password against a stored "salt:hash" string.
// Uses constant-time comparison to prevent timing attacks.
func VerifyPassword(stored, password string) bool {
	parts := strings.SplitN(stored, ":", 2)
	if len(parts) != 2 {
		return false
	}
	salt, err := hex.DecodeString(parts[0])
	if err != nil {
		return false
	}
	expected, err := hex.DecodeString(parts[1])
	if err != nil {
		return false
	}
	actual := deriveKey(password, salt, pbkdfIterations)
	return subtle.ConstantTimeCompare(expected, actual) == 1
}

// deriveKey computes PBKDF2-HMAC-SHA256(password, salt, iterations).
func deriveKey(password string, salt []byte, iterations int) []byte {
	// PBKDF2 with HMAC-SHA256 (RFC 2898)
	prf := func(key, data []byte) []byte {
		mac := hmac.New(sha256.New, key)
		mac.Write(data)
		return mac.Sum(nil)
	}

	// Single block derivation (32 bytes output)
	keyLen := sha256.Size
	block := make([]byte, len(salt)+4)
	copy(block, salt)
	block[len(salt)+0] = 0
	block[len(salt)+1] = 0
	block[len(salt)+2] = 0
	block[len(salt)+3] = 1

	u := prf([]byte(password), block)
	result := make([]byte, keyLen)
	copy(result, u)

	for i := 1; i < iterations; i++ {
		u = prf([]byte(password), u)
		for j := range result {
			result[j] ^= u[j]
		}
	}
	return result
}

// GenerateRandomString generates a URL-safe random string of n bytes (hex-encoded).
func GenerateRandomString(n int) (string, error) {
	b := make([]byte, n)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}

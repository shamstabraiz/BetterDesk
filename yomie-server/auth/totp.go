package auth

import (
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha1"
	"crypto/subtle"
	"encoding/base32"
	"encoding/binary"
	"fmt"
	"math"
	"net/url"
	"time"
)

const (
	totpPeriod = 30 // Time step in seconds (RFC 6238)
	totpDigits = 6  // Number of OTP digits
	totpWindow = 1  // ±1 time step tolerance for clock drift
)

// GenerateTOTPSecret creates a new random 20-byte TOTP secret (base32 encoded).
func GenerateTOTPSecret() string {
	secret := make([]byte, 20)
	rand.Read(secret)
	return base32.StdEncoding.WithPadding(base32.NoPadding).EncodeToString(secret)
}

// ComputeTOTP generates a TOTP code for the given secret and time.
func ComputeTOTP(secret string, t time.Time) (string, error) {
	key, err := base32.StdEncoding.WithPadding(base32.NoPadding).DecodeString(secret)
	if err != nil {
		return "", fmt.Errorf("auth: decode TOTP secret: %w", err)
	}

	counter := uint64(t.Unix()) / totpPeriod
	buf := make([]byte, 8)
	binary.BigEndian.PutUint64(buf, counter)

	mac := hmac.New(sha1.New, key)
	mac.Write(buf)
	hash := mac.Sum(nil)

	// Dynamic truncation (RFC 4226 §5.3)
	offset := hash[len(hash)-1] & 0x0f
	code := binary.BigEndian.Uint32(hash[offset:offset+4]) & 0x7fffffff
	code = code % uint32(math.Pow10(totpDigits))

	return fmt.Sprintf("%0*d", totpDigits, code), nil
}

// ValidateTOTP checks a TOTP code against the secret, allowing ±1 time step.
// Uses constant-time comparison per step to prevent timing attacks.
func ValidateTOTP(secret, code string) bool {
	if len(code) != totpDigits {
		return false
	}
	now := time.Now()
	for i := -totpWindow; i <= totpWindow; i++ {
		t := now.Add(time.Duration(i*totpPeriod) * time.Second)
		expected, err := ComputeTOTP(secret, t)
		if err != nil {
			continue
		}
		if subtle.ConstantTimeCompare([]byte(expected), []byte(code)) == 1 {
			return true
		}
	}
	return false
}

// TOTPUri generates an otpauth:// URI for QR code generation.
// Compatible with Google Authenticator, Authy, and other TOTP apps.
func TOTPUri(secret, issuer, account string) string {
	return fmt.Sprintf("otpauth://totp/%s:%s?secret=%s&issuer=%s&digits=%d&period=%d",
		url.PathEscape(issuer),
		url.PathEscape(account),
		secret,
		url.QueryEscape(issuer),
		totpDigits,
		totpPeriod,
	)
}

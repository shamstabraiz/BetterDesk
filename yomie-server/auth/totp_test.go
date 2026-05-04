package auth

import (
	"testing"
	"time"
)

func TestGenerateTOTPSecret(t *testing.T) {
	s1 := GenerateTOTPSecret()
	s2 := GenerateTOTPSecret()
	if s1 == s2 {
		t.Error("Two secrets should differ")
	}
	if len(s1) != 32 { // 20 bytes → 32 base32 chars (no padding)
		t.Errorf("Expected 32 chars, got %d", len(s1))
	}
}

func TestComputeTOTP(t *testing.T) {
	// RFC 6238 test vector: secret = "12345678901234567890" (ASCII)
	// At Unix time 59, step=30 → counter=1, SHA1 → "94287082" (8 digits)
	// We use 6 digits so we take last 6: "287082"
	secret := "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ" // base32 of "12345678901234567890"
	testTime := time.Unix(59, 0)
	code, err := ComputeTOTP(secret, testTime)
	if err != nil {
		t.Fatalf("ComputeTOTP: %v", err)
	}
	if len(code) != 6 {
		t.Errorf("Expected 6-digit code, got %q", code)
	}
	// The code should be deterministic
	code2, _ := ComputeTOTP(secret, testTime)
	if code != code2 {
		t.Error("Same time should produce same code")
	}
}

func TestValidateTOTP(t *testing.T) {
	secret := GenerateTOTPSecret()
	code, err := ComputeTOTP(secret, time.Now())
	if err != nil {
		t.Fatalf("ComputeTOTP: %v", err)
	}
	if !ValidateTOTP(secret, code) {
		t.Error("Current code should validate")
	}
	if ValidateTOTP(secret, "000000") && code != "000000" {
		t.Error("Wrong code should not validate")
	}
}

func TestValidateTOTPWindow(t *testing.T) {
	secret := GenerateTOTPSecret()
	// Code from 30 seconds ago should still validate (within window)
	pastCode, _ := ComputeTOTP(secret, time.Now().Add(-30*time.Second))
	if !ValidateTOTP(secret, pastCode) {
		t.Error("Code from -30s should validate within ±1 window")
	}
}

func TestValidateTOTPBadLength(t *testing.T) {
	secret := GenerateTOTPSecret()
	if ValidateTOTP(secret, "12345") {
		t.Error("5-digit code should fail")
	}
	if ValidateTOTP(secret, "1234567") {
		t.Error("7-digit code should fail")
	}
}

func TestTOTPUri(t *testing.T) {
	uri := TOTPUri("ABCDEFGH", "Yomie", "admin")
	if uri == "" {
		t.Error("URI should not be empty")
	}
	expected := "otpauth://totp/Yomie:admin?secret=ABCDEFGH&issuer=Yomie&digits=6&period=30"
	if uri != expected {
		t.Errorf("URI mismatch:\n  got:  %s\n  want: %s", uri, expected)
	}
}

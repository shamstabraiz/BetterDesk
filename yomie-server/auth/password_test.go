package auth

import (
	"testing"
)

func TestHashAndVerify(t *testing.T) {
	password := "SuperSecret123!"
	hash, err := HashPassword(password)
	if err != nil {
		t.Fatalf("HashPassword: %v", err)
	}

	if !VerifyPassword(hash, password) {
		t.Error("VerifyPassword should return true for correct password")
	}

	if VerifyPassword(hash, "WrongPassword") {
		t.Error("VerifyPassword should return false for wrong password")
	}
}

func TestHashUniqueness(t *testing.T) {
	h1, _ := HashPassword("same")
	h2, _ := HashPassword("same")
	if h1 == h2 {
		t.Error("Two hashes of the same password should differ (different salts)")
	}
}

func TestVerifyMalformedHash(t *testing.T) {
	if VerifyPassword("", "test") {
		t.Error("Empty hash should fail")
	}
	if VerifyPassword("noseparator", "test") {
		t.Error("Hash without separator should fail")
	}
	if VerifyPassword("zzzz:yyyy", "test") {
		t.Error("Invalid hex should fail")
	}
}

func TestGenerateRandomString(t *testing.T) {
	s1, err := GenerateRandomString(32)
	if err != nil {
		t.Fatalf("GenerateRandomString: %v", err)
	}
	s2, _ := GenerateRandomString(32)
	if s1 == s2 {
		t.Error("Two random strings should differ")
	}
	if len(s1) != 64 { // 32 bytes → 64 hex chars
		t.Errorf("Expected 64 hex chars, got %d", len(s1))
	}
}

package auth

import (
	"strings"
	"testing"
	"time"
)

func TestJWTGenerateAndValidate(t *testing.T) {
	mgr := NewJWTManager("test-secret-key-32bytes-minimum!", 1*time.Hour)

	token, err := mgr.Generate("admin", "admin")
	if err != nil {
		t.Fatalf("Generate: %v", err)
	}

	parts := strings.SplitN(token, ".", 3)
	if len(parts) != 3 {
		t.Fatalf("Expected 3 parts, got %d", len(parts))
	}

	claims, err := mgr.Validate(token)
	if err != nil {
		t.Fatalf("Validate: %v", err)
	}
	if claims.Sub != "admin" {
		t.Errorf("Expected sub=admin, got %q", claims.Sub)
	}
	if claims.Role != "admin" {
		t.Errorf("Expected role=admin, got %q", claims.Role)
	}
	if claims.Jti == "" {
		t.Error("Expected non-empty jti")
	}
}

func TestJWTExpired(t *testing.T) {
	mgr := NewJWTManager("test-secret", 0) // 0 expiry = already expired

	token, _ := mgr.Generate("user", "viewer")
	time.Sleep(2 * time.Second)
	_, err := mgr.Validate(token)
	if err != ErrTokenExpired {
		t.Errorf("Expected ErrTokenExpired, got %v", err)
	}
}

func TestJWTInvalidSignature(t *testing.T) {
	mgr1 := NewJWTManager("secret-one", 1*time.Hour)
	mgr2 := NewJWTManager("secret-two", 1*time.Hour)

	token, _ := mgr1.Generate("user", "admin")
	_, err := mgr2.Validate(token)
	if err != ErrInvalidToken {
		t.Errorf("Expected ErrInvalidToken, got %v", err)
	}
}

func TestJWTMalformed(t *testing.T) {
	mgr := NewJWTManager("secret", 1*time.Hour)

	tests := []string{
		"",
		"one",
		"one.two",
		"one.two.three",
	}
	for _, tok := range tests {
		_, err := mgr.Validate(tok)
		if err == nil {
			t.Errorf("Expected error for token %q", tok)
		}
	}
}

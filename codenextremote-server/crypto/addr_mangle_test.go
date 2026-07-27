package crypto

import (
	"net"
	"testing"
)

func TestEncodeDecodeIPv4(t *testing.T) {
	addr := &net.UDPAddr{
		IP:   net.ParseIP("192.168.1.100"),
		Port: 21116,
	}
	encoded := EncodeAddr(addr)

	if len(encoded) != 6 {
		t.Fatalf("expected 6 bytes for IPv4, got %d", len(encoded))
	}

	// Verify raw bytes
	if encoded[0] != 192 || encoded[1] != 168 || encoded[2] != 1 || encoded[3] != 100 {
		t.Errorf("unexpected IP bytes: %v", encoded[0:4])
	}
	// Port 21116 = 0x527C big-endian
	if encoded[4] != 0x52 || encoded[5] != 0x7C {
		t.Errorf("unexpected port bytes: %02X %02X", encoded[4], encoded[5])
	}

	decoded, err := DecodeAddr(encoded)
	if err != nil {
		t.Fatalf("decode error: %v", err)
	}

	if !decoded.IP.Equal(addr.IP) {
		t.Errorf("IP mismatch: got %v, want %v", decoded.IP, addr.IP)
	}
	if decoded.Port != addr.Port {
		t.Errorf("port mismatch: got %d, want %d", decoded.Port, addr.Port)
	}
}

func TestEncodeDecodeIPv6(t *testing.T) {
	addr := &net.UDPAddr{
		IP:   net.ParseIP("2001:db8::1"),
		Port: 443,
	}
	encoded := EncodeAddr(addr)

	if len(encoded) != 18 {
		t.Fatalf("expected 18 bytes for IPv6, got %d", len(encoded))
	}

	decoded, err := DecodeAddr(encoded)
	if err != nil {
		t.Fatalf("decode error: %v", err)
	}

	if !decoded.IP.Equal(addr.IP) {
		t.Errorf("IP mismatch: got %v, want %v", decoded.IP, addr.IP)
	}
	if decoded.Port != addr.Port {
		t.Errorf("port mismatch: got %d, want %d", decoded.Port, addr.Port)
	}
}

func TestEncodeDecodeIPv4MappedIPv6(t *testing.T) {
	// IPv4-mapped IPv6 like ::ffff:192.168.1.1 should encode as IPv4 (4 bytes)
	addr := &net.UDPAddr{
		IP:   net.ParseIP("::ffff:192.168.1.1"),
		Port: 8080,
	}
	encoded := EncodeAddr(addr)

	// To4() returns non-nil for IPv4-mapped — so we expect 6 bytes
	if len(encoded) != 6 {
		t.Fatalf("expected 6 bytes for IPv4-mapped IPv6, got %d", len(encoded))
	}

	decoded, err := DecodeAddr(encoded)
	if err != nil {
		t.Fatalf("decode error: %v", err)
	}

	expected := net.ParseIP("192.168.1.1").To4()
	if !decoded.IP.Equal(expected) {
		t.Errorf("IP mismatch: got %v, want %v", decoded.IP, expected)
	}
	if decoded.Port != 8080 {
		t.Errorf("port mismatch: got %d, want 8080", decoded.Port)
	}
}

func TestDecodeInvalidLength(t *testing.T) {
	testCases := []int{0, 1, 4, 5, 7, 10, 17, 19, 100}
	for _, n := range testCases {
		b := make([]byte, n)
		_, err := DecodeAddr(b)
		if err == nil {
			t.Errorf("expected error for length %d, got nil", n)
		}
	}
}

func TestEncodeDecodeString(t *testing.T) {
	encoded, err := EncodeAddrFromString("10.0.0.1:5000")
	if err != nil {
		t.Fatalf("encode error: %v", err)
	}

	s, err := DecodeAddrToString(encoded)
	if err != nil {
		t.Fatalf("decode error: %v", err)
	}

	if s != "10.0.0.1:5000" {
		t.Errorf("got %q, want %q", s, "10.0.0.1:5000")
	}
}

func TestEncodeDecodePort0(t *testing.T) {
	addr := &net.UDPAddr{IP: net.ParseIP("0.0.0.0"), Port: 0}
	encoded := EncodeAddr(addr)
	decoded, err := DecodeAddr(encoded)
	if err != nil {
		t.Fatalf("decode error: %v", err)
	}
	if decoded.Port != 0 {
		t.Errorf("expected port 0, got %d", decoded.Port)
	}
}

func TestEncodeDecodeMaxPort(t *testing.T) {
	addr := &net.UDPAddr{IP: net.ParseIP("255.255.255.255"), Port: 65535}
	encoded := EncodeAddr(addr)
	decoded, err := DecodeAddr(encoded)
	if err != nil {
		t.Fatalf("decode error: %v", err)
	}
	if decoded.Port != 65535 {
		t.Errorf("expected port 65535, got %d", decoded.Port)
	}
	if !decoded.IP.Equal(net.ParseIP("255.255.255.255")) {
		t.Errorf("IP mismatch: got %v", decoded.IP)
	}
}

func BenchmarkEncodeIPv4(b *testing.B) {
	addr := &net.UDPAddr{IP: net.ParseIP("192.168.1.1"), Port: 21116}
	for i := 0; i < b.N; i++ {
		EncodeAddr(addr)
	}
}

func BenchmarkDecodeIPv4(b *testing.B) {
	data := []byte{192, 168, 1, 1, 0x52, 0x7C}
	for i := 0; i < b.N; i++ {
		DecodeAddr(data)
	}
}

package config

import (
	"bufio"
	"crypto/ecdsa"
	"crypto/elliptic"
	"crypto/rand"
	"crypto/tls"
	"crypto/x509"
	"crypto/x509/pkix"
	"encoding/pem"
	"math/big"
	"net"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// generateTestCert creates a self-signed TLS certificate for testing.
// Returns paths to cert.pem and key.pem files in a temp directory.
func generateTestCert(t *testing.T) (certFile, keyFile string) {
	t.Helper()

	privKey, err := ecdsa.GenerateKey(elliptic.P256(), rand.Reader)
	if err != nil {
		t.Fatalf("generate key: %v", err)
	}

	tmpl := &x509.Certificate{
		SerialNumber: big.NewInt(1),
		Subject:      pkix.Name{CommonName: "test"},
		NotBefore:    time.Now().Add(-time.Hour),
		NotAfter:     time.Now().Add(time.Hour),
		KeyUsage:     x509.KeyUsageDigitalSignature | x509.KeyUsageKeyEncipherment,
		ExtKeyUsage:  []x509.ExtKeyUsage{x509.ExtKeyUsageServerAuth},
		IPAddresses:  []net.IP{net.ParseIP("127.0.0.1")},
	}

	certDER, err := x509.CreateCertificate(rand.Reader, tmpl, tmpl, &privKey.PublicKey, privKey)
	if err != nil {
		t.Fatalf("create cert: %v", err)
	}

	dir := t.TempDir()
	certFile = filepath.Join(dir, "cert.pem")
	keyFile = filepath.Join(dir, "key.pem")

	certPEM := pem.EncodeToMemory(&pem.Block{Type: "CERTIFICATE", Bytes: certDER})
	if err := os.WriteFile(certFile, certPEM, 0600); err != nil {
		t.Fatalf("write cert: %v", err)
	}

	keyDER, err := x509.MarshalECPrivateKey(privKey)
	if err != nil {
		t.Fatalf("marshal key: %v", err)
	}
	keyPEM := pem.EncodeToMemory(&pem.Block{Type: "EC PRIVATE KEY", Bytes: keyDER})
	if err := os.WriteFile(keyFile, keyPEM, 0600); err != nil {
		t.Fatalf("write key: %v", err)
	}

	return certFile, keyFile
}

func TestLoadTLSConfig(t *testing.T) {
	certFile, keyFile := generateTestCert(t)

	cfg, err := LoadTLSConfig(certFile, keyFile)
	if err != nil {
		t.Fatalf("LoadTLSConfig: %v", err)
	}
	if cfg == nil {
		t.Fatal("LoadTLSConfig returned nil")
	}
	if cfg.MinVersion != tls.VersionTLS12 {
		t.Errorf("MinVersion = %d, want %d", cfg.MinVersion, tls.VersionTLS12)
	}
	if len(cfg.Certificates) != 1 {
		t.Errorf("Certificates len = %d, want 1", len(cfg.Certificates))
	}
}

func TestLoadTLSConfig_InvalidPath(t *testing.T) {
	_, err := LoadTLSConfig("/nonexistent/cert.pem", "/nonexistent/key.pem")
	if err == nil {
		t.Fatal("expected error for invalid cert paths")
	}
}

func TestDualModeListener_PlainConn(t *testing.T) {
	certFile, keyFile := generateTestCert(t)
	tlsCfg, err := LoadTLSConfig(certFile, keyFile)
	if err != nil {
		t.Fatalf("LoadTLSConfig: %v", err)
	}

	// Create a plain TCP listener and wrap with DualModeListener
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer ln.Close()

	dualLn := NewDualModeListener(ln, tlsCfg)

	// Send a plain TCP message (first byte != 0x16)
	go func() {
		conn, err := net.Dial("tcp", ln.Addr().String())
		if err != nil {
			t.Errorf("dial: %v", err)
			return
		}
		defer conn.Close()
		conn.Write([]byte("hello plain"))
	}()

	conn, err := dualLn.Accept()
	if err != nil {
		t.Fatalf("accept: %v", err)
	}
	defer conn.Close()

	// Should be a plain connection (peekedConn), not TLS
	if _, ok := conn.(*tls.Conn); ok {
		t.Error("expected plain connection, got TLS")
	}

	buf := make([]byte, 32)
	conn.SetReadDeadline(time.Now().Add(2 * time.Second))
	n, err := conn.Read(buf)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(buf[:n]) != "hello plain" {
		t.Errorf("got %q, want %q", string(buf[:n]), "hello plain")
	}
}

func TestDualModeListener_TLSConn(t *testing.T) {
	certFile, keyFile := generateTestCert(t)
	tlsCfg, err := LoadTLSConfig(certFile, keyFile)
	if err != nil {
		t.Fatalf("LoadTLSConfig: %v", err)
	}

	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer ln.Close()

	dualLn := NewDualModeListener(ln, tlsCfg)

	// Connect with TLS (first byte will be 0x16 ClientHello)
	go func() {
		clientCfg := &tls.Config{InsecureSkipVerify: true}
		conn, err := tls.Dial("tcp", ln.Addr().String(), clientCfg)
		if err != nil {
			t.Errorf("tls dial: %v", err)
			return
		}
		defer conn.Close()
		conn.Write([]byte("hello tls"))
	}()

	conn, err := dualLn.Accept()
	if err != nil {
		t.Fatalf("accept: %v", err)
	}
	defer conn.Close()

	// Should be a TLS connection
	tlsConn, ok := conn.(*tls.Conn)
	if !ok {
		t.Fatal("expected TLS connection, got plain")
	}

	// Complete the TLS handshake
	tlsConn.SetDeadline(time.Now().Add(5 * time.Second))
	if err := tlsConn.Handshake(); err != nil {
		t.Fatalf("handshake: %v", err)
	}

	buf := make([]byte, 32)
	n, err := tlsConn.Read(buf)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	if string(buf[:n]) != "hello tls" {
		t.Errorf("got %q, want %q", string(buf[:n]), "hello tls")
	}
}

func TestDualModeListener_Addr(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer ln.Close()

	dualLn := NewDualModeListener(ln, &tls.Config{})
	if dualLn.Addr() != ln.Addr() {
		t.Errorf("Addr() = %v, want %v", dualLn.Addr(), ln.Addr())
	}
}

func TestPeekedConn_Read(t *testing.T) {
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("listen: %v", err)
	}
	defer ln.Close()

	go func() {
		conn, _ := net.Dial("tcp", ln.Addr().String())
		conn.Write([]byte("abcdef"))
		conn.Close()
	}()

	conn, err := ln.Accept()
	if err != nil {
		t.Fatalf("accept: %v", err)
	}
	defer conn.Close()

	br := bufio.NewReaderSize(conn, 1)
	// Peek first byte
	peeked, err := br.Peek(1)
	if err != nil {
		t.Fatalf("peek: %v", err)
	}
	if peeked[0] != 'a' {
		t.Errorf("peeked byte = %q, want 'a'", peeked[0])
	}

	// Wrap and read — should include the peeked byte
	pc := &peekedConn{Conn: conn, reader: br}
	buf := make([]byte, 32)
	pc.SetReadDeadline(time.Now().Add(2 * time.Second))
	n, err := pc.Read(buf)
	if err != nil {
		t.Fatalf("read: %v", err)
	}
	got := string(buf[:n])
	if got[0] != 'a' {
		t.Errorf("first byte after peek = %q, want 'a'", got[0])
	}
}

func TestConfigHelpers(t *testing.T) {
	c := &Config{}
	if c.HasTLSCert() {
		t.Error("HasTLSCert should be false without cert/key")
	}
	if c.SignalTLSEnabled() {
		t.Error("SignalTLSEnabled should be false")
	}
	if c.RelayTLSEnabled() {
		t.Error("RelayTLSEnabled should be false")
	}

	c.TLSCertFile = "/tmp/cert.pem"
	c.TLSKeyFile = "/tmp/key.pem"
	if !c.HasTLSCert() {
		t.Error("HasTLSCert should be true")
	}
	if c.SignalTLSEnabled() {
		t.Error("SignalTLSEnabled should be false without TLSSignal flag")
	}

	c.TLSSignal = true
	if !c.SignalTLSEnabled() {
		t.Error("SignalTLSEnabled should be true")
	}

	c.TLSRelay = true
	if !c.RelayTLSEnabled() {
		t.Error("RelayTLSEnabled should be true")
	}
}

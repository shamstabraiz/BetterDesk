package api

import (
	"context"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/base64"
	"fmt"
	"net/http"
	"testing"
	"time"

	"github.com/coder/websocket"
	"github.com/unitronix/betterdesk-server/config"
	"github.com/unitronix/betterdesk-server/db"
	"github.com/unitronix/betterdesk-server/peer"
)

func signedBdMgmtHeaders(t *testing.T, privateKey ed25519.PrivateKey, deviceID string) http.Header {
	t.Helper()
	timestamp := time.Now().UTC().Format(time.RFC3339)
	nonce := fmt.Sprintf("nonce-%d", time.Now().UnixNano())
	payload := bdMgmtSignaturePayload(deviceID, timestamp, nonce)
	signature := ed25519.Sign(privateKey, payload)

	headers := make(http.Header)
	headers.Set("X-BD-Timestamp", timestamp)
	headers.Set("X-BD-Nonce", nonce)
	headers.Set("X-BD-Signature", base64.StdEncoding.EncodeToString(signature))
	return headers
}

func applyHeaders(req *http.Request, headers http.Header) {
	for key, values := range headers {
		for _, value := range values {
			req.Header.Add(key, value)
		}
	}
}

func TestVerifyBdMgmtRequestAcceptsValidSignature(t *testing.T) {
	database := testSetupDB(t)
	defer database.Close()

	pub, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	if err := database.UpsertPeer(&db.Peer{ID: "BDM001", PK: pub, Status: "ONLINE"}); err != nil {
		t.Fatal(err)
	}

	srv := New(config.DefaultConfig(), database, peer.NewMap(), nil, "test")
	req, err := http.NewRequest("GET", "http://example.test/ws/bd-mgmt/BDM001", nil)
	if err != nil {
		t.Fatal(err)
	}
	applyHeaders(req, signedBdMgmtHeaders(t, priv, "BDM001"))

	if err := srv.verifyBdMgmtRequest(req, "BDM001"); err != nil {
		t.Fatalf("verifyBdMgmtRequest returned error: %v", err)
	}
}

func TestVerifyBdMgmtRequestRejectsReplay(t *testing.T) {
	database := testSetupDB(t)
	defer database.Close()

	pub, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	if err := database.UpsertPeer(&db.Peer{ID: "BDM002", PK: pub, Status: "ONLINE"}); err != nil {
		t.Fatal(err)
	}

	srv := New(config.DefaultConfig(), database, peer.NewMap(), nil, "test")
	headers := signedBdMgmtHeaders(t, priv, "BDM002")

	firstReq, err := http.NewRequest("GET", "http://example.test/ws/bd-mgmt/BDM002", nil)
	if err != nil {
		t.Fatal(err)
	}
	applyHeaders(firstReq, headers)
	if err := srv.verifyBdMgmtRequest(firstReq, "BDM002"); err != nil {
		t.Fatalf("first verifyBdMgmtRequest returned error: %v", err)
	}

	replayReq, err := http.NewRequest("GET", "http://example.test/ws/bd-mgmt/BDM002", nil)
	if err != nil {
		t.Fatal(err)
	}
	applyHeaders(replayReq, headers)
	if err := srv.verifyBdMgmtRequest(replayReq, "BDM002"); err == nil {
		t.Fatal("expected replayed request to be rejected")
	}
}

func TestBdMgmtWebSocketRequiresProof(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.APIPort = 19886
	database := testSetupDB(t)
	defer database.Close()

	pub, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	if err := database.UpsertPeer(&db.Peer{ID: "BDM003", PK: pub, Status: "ONLINE"}); err != nil {
		t.Fatal(err)
	}

	srv := New(cfg, database, peer.NewMap(), nil, "test")
	if err := srv.Start(t.Context()); err != nil {
		t.Fatal(err)
	}
	defer srv.Stop()
	time.Sleep(100 * time.Millisecond)

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	wsURL := fmt.Sprintf("ws://127.0.0.1:%d/ws/bd-mgmt/BDM003", cfg.APIPort)

	unauthConn, resp, err := websocket.Dial(ctx, wsURL, nil)
	if err == nil {
		unauthConn.Close(websocket.StatusPolicyViolation, "expected unauthorized")
		t.Fatal("expected unsigned management websocket to be rejected")
	}
	if resp == nil || resp.StatusCode != http.StatusUnauthorized {
		status := 0
		if resp != nil {
			status = resp.StatusCode
		}
		t.Fatalf("expected 401 for unsigned websocket, got %d (err=%v)", status, err)
	}

	conn, resp, err := websocket.Dial(ctx, wsURL, &websocket.DialOptions{
		HTTPHeader: signedBdMgmtHeaders(t, priv, "BDM003"),
	})
	if err != nil {
		status := 0
		if resp != nil {
			status = resp.StatusCode
		}
		t.Fatalf("expected signed websocket to connect, got status %d: %v", status, err)
	}
	_ = conn.Close(websocket.StatusNormalClosure, "done")
}

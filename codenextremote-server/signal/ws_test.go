package signal

import (
	"context"
	"testing"
	"time"

	"github.com/coder/websocket"
	"github.com/unitronix/betterdesk-server/config"
	"github.com/unitronix/betterdesk-server/crypto"
	"github.com/unitronix/betterdesk-server/db"
	pb "github.com/unitronix/betterdesk-server/proto"
	"google.golang.org/protobuf/proto"
)

func TestWSSignalHealthCheck(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.SignalPort = 29100
	cfg.RelayPort = 29101

	dir := t.TempDir()
	cfg.DBPath = dir + "/test.db"
	cfg.KeyFile = dir + "/id_ed25519"

	database, err := db.OpenSQLite(cfg.DBPath)
	if err != nil {
		t.Fatal(err)
	}
	database.Migrate()
	defer database.Close()

	kp, err := crypto.LoadOrGenerateKeyPair(cfg.KeyFile)
	if err != nil {
		t.Fatal(err)
	}

	srv := New(cfg, kp, database)
	ctx := t.Context()
	if err := srv.Start(ctx); err != nil {
		t.Fatal(err)
	}
	defer srv.Stop()
	time.Sleep(200 * time.Millisecond)

	// Connect via WebSocket to signal port + 2 = 29102
	wsURL := "ws://127.0.0.1:29102/"
	ws, _, err := websocket.Dial(ctx, wsURL, nil)
	if err != nil {
		t.Fatalf("WS dial: %v", err)
	}
	defer ws.CloseNow()

	// Send health check
	hc := &pb.RendezvousMessage{
		Union: &pb.RendezvousMessage_Hc{
			Hc: &pb.HealthCheck{Token: "ws-test-123"},
		},
	}
	data, _ := proto.Marshal(hc)
	if err := ws.Write(ctx, websocket.MessageBinary, data); err != nil {
		t.Fatalf("WS write: %v", err)
	}

	// Read response
	_, respData, err := ws.Read(ctx)
	if err != nil {
		t.Fatalf("WS read: %v", err)
	}

	resp := &pb.RendezvousMessage{}
	if err := proto.Unmarshal(respData, resp); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}

	if resp.GetHc() == nil || resp.GetHc().Token != "ws-test-123" {
		t.Errorf("unexpected response: %v", resp)
	}
}

func TestWSSignalRegisterPeer(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.SignalPort = 29200
	cfg.RelayPort = 29201

	dir := t.TempDir()
	cfg.DBPath = dir + "/test.db"
	cfg.KeyFile = dir + "/id_ed25519"

	database, err := db.OpenSQLite(cfg.DBPath)
	if err != nil {
		t.Fatal(err)
	}
	database.Migrate()
	defer database.Close()

	kp, err := crypto.LoadOrGenerateKeyPair(cfg.KeyFile)
	if err != nil {
		t.Fatal(err)
	}

	srv := New(cfg, kp, database)
	ctx := t.Context()
	if err := srv.Start(ctx); err != nil {
		t.Fatal(err)
	}
	defer srv.Stop()
	time.Sleep(200 * time.Millisecond)

	// Connect WS
	ws, _, err := websocket.Dial(ctx, "ws://127.0.0.1:29202/", nil)
	if err != nil {
		t.Fatalf("WS dial: %v", err)
	}
	defer ws.CloseNow()

	// Send RegisterPeer (heartbeat)
	reg := &pb.RendezvousMessage{
		Union: &pb.RendezvousMessage_RegisterPeer{
			RegisterPeer: &pb.RegisterPeer{
				Id:     "WSTEST1",
				Serial: 1,
			},
		},
	}
	data, _ := proto.Marshal(reg)
	ws.Write(ctx, websocket.MessageBinary, data)

	// Read response
	_, respData, err := ws.Read(ctx)
	if err != nil {
		t.Fatalf("WS read: %v", err)
	}

	resp := &pb.RendezvousMessage{}
	proto.Unmarshal(respData, resp)

	rpr := resp.GetRegisterPeerResponse()
	if rpr == nil {
		t.Fatalf("expected RegisterPeerResponse, got: %v", resp)
	}
	if !rpr.RequestPk {
		t.Error("should request PK for new peer")
	}

	// Verify peer is in memory
	if !srv.PeerMap().IsOnline("WSTEST1", config.RegTimeout) {
		t.Error("peer WSTEST1 should be online")
	}
}

func TestWSSignalOnlineRequest(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.SignalPort = 29300
	cfg.RelayPort = 29301

	dir := t.TempDir()
	cfg.DBPath = dir + "/test.db"
	cfg.KeyFile = dir + "/id_ed25519"

	database, err := db.OpenSQLite(cfg.DBPath)
	if err != nil {
		t.Fatal(err)
	}
	database.Migrate()
	defer database.Close()

	kp, err := crypto.LoadOrGenerateKeyPair(cfg.KeyFile)
	if err != nil {
		t.Fatal(err)
	}

	srv := New(cfg, kp, database)
	ctx, cancel := context.WithTimeout(t.Context(), 10*time.Second)
	defer cancel()

	if err := srv.Start(ctx); err != nil {
		t.Fatal(err)
	}
	defer srv.Stop()
	time.Sleep(200 * time.Millisecond)

	// First register a peer via one WS connection
	ws1, _, _ := websocket.Dial(ctx, "ws://127.0.0.1:29302/", nil)
	defer ws1.CloseNow()

	reg := &pb.RendezvousMessage{
		Union: &pb.RendezvousMessage_RegisterPeer{
			RegisterPeer: &pb.RegisterPeer{Id: "ONLINE1", Serial: 1},
		},
	}
	data, _ := proto.Marshal(reg)
	ws1.Write(ctx, websocket.MessageBinary, data)
	ws1.Read(ctx) // consume RegisterPeerResponse

	// Now query online status via WS
	ws2, _, _ := websocket.Dial(ctx, "ws://127.0.0.1:29302/", nil)
	defer ws2.CloseNow()

	online := &pb.RendezvousMessage{
		Union: &pb.RendezvousMessage_OnlineRequest{
			OnlineRequest: &pb.OnlineRequest{
				Peers: []string{"ONLINE1", "NOTEXIST"},
			},
		},
	}
	data, _ = proto.Marshal(online)
	ws2.Write(ctx, websocket.MessageBinary, data)

	_, respData, err := ws2.Read(ctx)
	if err != nil {
		t.Fatalf("WS read: %v", err)
	}
	resp := &pb.RendezvousMessage{}
	proto.Unmarshal(respData, resp)

	or := resp.GetOnlineResponse()
	if or == nil {
		t.Fatalf("expected OnlineResponse, got: %v", resp)
	}

	// 1-bit-per-peer, big-endian: ONLINE1 (index 0) → bit 7 → 0x80
	if len(or.States) == 0 || or.States[0]&0x80 == 0 {
		t.Errorf("ONLINE1 should be online (bit 7), states: %v", or.States)
	}
	// NOTEXIST (index 1) → bit 6 → should be 0
	if len(or.States) > 0 && or.States[0]&0x40 != 0 {
		t.Errorf("NOTEXIST should be offline (bit 6), states: %v", or.States)
	}
}

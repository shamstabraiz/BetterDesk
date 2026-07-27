package relay

import (
	"fmt"
	"testing"
	"time"

	"github.com/coder/websocket"
	"github.com/unitronix/betterdesk-server/config"
	pb "github.com/unitronix/betterdesk-server/proto"
	"google.golang.org/protobuf/proto"
)

func TestWSRelayHealthCheck(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.RelayPort = 29400

	srv := New(cfg)
	ctx := t.Context()
	if err := srv.Start(ctx); err != nil {
		t.Fatal(err)
	}
	defer srv.Stop()
	time.Sleep(200 * time.Millisecond)

	// Connect to WS relay port (relay port + 2 = 29402)
	wsURL := fmt.Sprintf("ws://127.0.0.1:%d/", cfg.WSRelayPort())
	ws, _, err := websocket.Dial(ctx, wsURL, nil)
	if err != nil {
		t.Fatalf("WS dial: %v", err)
	}
	defer ws.CloseNow()

	// Send health check
	hc := &pb.RendezvousMessage{
		Union: &pb.RendezvousMessage_Hc{
			Hc: &pb.HealthCheck{Token: "relay-ws-test"},
		},
	}
	data, _ := proto.Marshal(hc)
	ws.Write(ctx, websocket.MessageBinary, data)

	// Read response
	_, respData, err := ws.Read(ctx)
	if err != nil {
		t.Fatalf("WS read: %v", err)
	}

	resp := &pb.RendezvousMessage{}
	proto.Unmarshal(respData, resp)

	if resp.GetHc() == nil || resp.GetHc().Token != "relay-ws-test" {
		t.Errorf("unexpected response: %v", resp)
	}
}

func TestWSRelayPairing(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.RelayPort = 29500

	srv := New(cfg)
	ctx := t.Context()
	if err := srv.Start(ctx); err != nil {
		t.Fatal(err)
	}
	defer srv.Stop()
	time.Sleep(200 * time.Millisecond)

	uuid := "ws-relay-test-uuid-456"
	wsURL := fmt.Sprintf("ws://127.0.0.1:%d/", cfg.WSRelayPort())

	// Connect first side
	ws1, _, err := websocket.Dial(ctx, wsURL, nil)
	if err != nil {
		t.Fatalf("WS dial 1: %v", err)
	}
	defer ws1.CloseNow()

	// Send RequestRelay from first side
	rr := &pb.RendezvousMessage{
		Union: &pb.RendezvousMessage_RequestRelay{
			RequestRelay: &pb.RequestRelay{Uuid: uuid},
		},
	}
	data, _ := proto.Marshal(rr)
	ws1.Write(ctx, websocket.MessageBinary, data)

	// Small delay before second connection
	time.Sleep(50 * time.Millisecond)

	// Connect second side
	ws2, _, err := websocket.Dial(ctx, wsURL, nil)
	if err != nil {
		t.Fatalf("WS dial 2: %v", err)
	}
	defer ws2.CloseNow()

	// Send RequestRelay from second side with same UUID
	ws2.Write(ctx, websocket.MessageBinary, data)

	// Both sides should receive RelayResponse confirmation via the net.Conn adapter.
	// Since websocket.NetConn wraps binary messages, the framed RelayResponse
	// arrives as codec.WriteRawFrame bytes. We just verify the connection pair
	// was established by checking stats.
	time.Sleep(300 * time.Millisecond)

	if srv.TotalRelayed.Load() < 1 {
		t.Errorf("expected at least 1 relay session, got %d", srv.TotalRelayed.Load())
	}
}

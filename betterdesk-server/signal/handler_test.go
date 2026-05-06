package signal

import (
	"bytes"
	"path/filepath"
	"testing"

	"github.com/unitronix/betterdesk-server/config"
	"github.com/unitronix/betterdesk-server/db"
	pb "github.com/unitronix/betterdesk-server/proto"
)

func newTestSignalServer(t *testing.T, mode string) (*Server, db.Database) {
	t.Helper()

	database, err := db.OpenSQLite(filepath.Join(t.TempDir(), "signal-test.db"))
	if err != nil {
		t.Fatalf("OpenSQLite: %v", err)
	}
	if err := database.Migrate(); err != nil {
		t.Fatalf("Migrate: %v", err)
	}
	t.Cleanup(func() { database.Close() })

	cfg := config.DefaultConfig()
	cfg.EnrollmentMode = mode
	return New(cfg, nil, database), database
}

func registerPkResult(resp *pb.RendezvousMessage) pb.RegisterPkResponse_Result {
	if resp == nil || resp.GetRegisterPkResponse() == nil {
		return pb.RegisterPkResponse_SERVER_ERROR
	}
	return resp.GetRegisterPkResponse().GetResult()
}

func newRegisterPk(peerID string) *pb.RegisterPk {
	return &pb.RegisterPk{
		Id:   peerID,
		Uuid: []byte("test-uuid-" + peerID),
		Pk:   bytes.Repeat([]byte{0x42}, 32),
	}
}

func TestProcessRegisterPkManagedRejectsUnknownPeer(t *testing.T) {
	srv, database := newTestSignalServer(t, config.EnrollmentModeManaged)

	resp := srv.processRegisterPk(newRegisterPk("NEWPK1"), "203.0.113.10:50123")
	if got := registerPkResult(resp); got != pb.RegisterPkResponse_NOT_SUPPORT {
		t.Fatalf("RegisterPk result = %v, want %v", got, pb.RegisterPkResponse_NOT_SUPPORT)
	}

	peer, err := database.GetPeer("NEWPK1")
	if err != nil {
		t.Fatalf("GetPeer: %v", err)
	}
	if peer != nil {
		t.Fatalf("unknown peer was persisted: %+v", peer)
	}
	if entry := srv.peers.Get("NEWPK1"); entry != nil {
		t.Fatalf("unknown peer remained in memory: %+v", entry)
	}
}

func TestHandleRegisterPeerWSManagedRejectsUnknownPeer(t *testing.T) {
	srv, database := newTestSignalServer(t, config.EnrollmentModeManaged)

	resp := srv.handleRegisterPeerWS(&pb.RegisterPeer{Id: "WSDENY1", Serial: 1}, "203.0.113.11:51234")
	if resp != nil {
		t.Fatalf("handleRegisterPeerWS returned response for rejected peer: %+v", resp)
	}
	if entry := srv.peers.Get("WSDENY1"); entry != nil {
		t.Fatalf("unknown WS peer remained in memory: %+v", entry)
	}
	peer, err := database.GetPeer("WSDENY1")
	if err != nil {
		t.Fatalf("GetPeer: %v", err)
	}
	if peer != nil {
		t.Fatalf("unknown WS peer was persisted: %+v", peer)
	}
}

func TestProcessRegisterPkManagedAllowsExistingPeer(t *testing.T) {
	srv, database := newTestSignalServer(t, config.EnrollmentModeManaged)

	if err := database.UpsertPeer(&db.Peer{ID: "KNOWN1", Status: "OFFLINE"}); err != nil {
		t.Fatalf("UpsertPeer: %v", err)
	}

	resp := srv.processRegisterPk(newRegisterPk("KNOWN1"), "203.0.113.10:50123")
	if got := registerPkResult(resp); got != pb.RegisterPkResponse_OK {
		t.Fatalf("RegisterPk result = %v, want %v", got, pb.RegisterPkResponse_OK)
	}

	peer, err := database.GetPeer("KNOWN1")
	if err != nil {
		t.Fatalf("GetPeer: %v", err)
	}
	if peer == nil || len(peer.PK) != 32 || peer.Status != "ONLINE" {
		t.Fatalf("existing peer was not updated correctly: %+v", peer)
	}
}

func TestProcessRegisterPkManagedAllowsTokenBoundPeer(t *testing.T) {
	srv, database := newTestSignalServer(t, config.EnrollmentModeManaged)

	token := &db.DeviceToken{
		Token:     "tokentest12345678",
		TokenHash: "token-hash-tokenpk1",
		Name:      "Token-bound stock peer",
		PeerID:    "TOKEN1",
		Status:    db.TokenStatusPending,
		MaxUses:   1,
	}
	if err := database.CreateDeviceToken(token); err != nil {
		t.Fatalf("CreateDeviceToken: %v", err)
	}

	resp := srv.processRegisterPk(newRegisterPk("TOKEN1"), "203.0.113.10:50123")
	if got := registerPkResult(resp); got != pb.RegisterPkResponse_OK {
		t.Fatalf("RegisterPk result = %v, want %v", got, pb.RegisterPkResponse_OK)
	}

	peer, err := database.GetPeer("TOKEN1")
	if err != nil {
		t.Fatalf("GetPeer: %v", err)
	}
	if peer == nil || len(peer.PK) != 32 {
		t.Fatalf("token-bound peer was not persisted with PK: %+v", peer)
	}
}

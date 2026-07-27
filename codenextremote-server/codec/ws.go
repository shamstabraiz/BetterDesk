// WebSocket framing adapter for RustDesk protobuf messages.
// WebSocket provides its own message framing, so we do NOT add
// the 2-byte length header used by TCP — raw protobuf goes directly
// into binary WS frames.
package codec

import (
	"context"
	"fmt"
	"net"

	"github.com/coder/websocket"
	pb "github.com/unitronix/betterdesk-server/proto"
	"google.golang.org/protobuf/proto"
)

// WSConn wraps a WebSocket connection for protobuf message I/O.
type WSConn struct {
	WS   *websocket.Conn
	Ctx  context.Context
	Addr string // remote address string (for logging)
}

// NewWSConn creates a WSConn from an accepted WebSocket connection.
func NewWSConn(ws *websocket.Conn, ctx context.Context, remoteAddr string) *WSConn {
	return &WSConn{WS: ws, Ctx: ctx, Addr: remoteAddr}
}

// ReadMessage reads one binary WS frame and decodes it as a RendezvousMessage.
func (c *WSConn) ReadMessage() (*pb.RendezvousMessage, error) {
	typ, data, err := c.WS.Read(c.Ctx)
	if err != nil {
		return nil, fmt.Errorf("ws read: %w", err)
	}
	if typ != websocket.MessageBinary {
		return nil, fmt.Errorf("ws: expected binary frame, got %v", typ)
	}

	msg := &pb.RendezvousMessage{}
	if err := proto.Unmarshal(data, msg); err != nil {
		return nil, fmt.Errorf("ws unmarshal: %w", err)
	}
	return msg, nil
}

// WriteMessage encodes a RendezvousMessage and sends it as a binary WS frame.
func (c *WSConn) WriteMessage(msg *pb.RendezvousMessage) error {
	data, err := proto.Marshal(msg)
	if err != nil {
		return fmt.Errorf("ws marshal: %w", err)
	}
	return c.WS.Write(c.Ctx, websocket.MessageBinary, data)
}

// WriteRaw sends raw bytes as a binary WS frame (for relay passthrough).
func (c *WSConn) WriteRaw(data []byte) error {
	return c.WS.Write(c.Ctx, websocket.MessageBinary, data)
}

// ReadRaw reads one binary WS frame and returns raw bytes.
func (c *WSConn) ReadRaw() ([]byte, error) {
	typ, data, err := c.WS.Read(c.Ctx)
	if err != nil {
		return nil, err
	}
	if typ != websocket.MessageBinary {
		return nil, fmt.Errorf("ws: expected binary frame, got %v", typ)
	}
	return data, nil
}

// RemoteAddr returns the remote address string.
func (c *WSConn) RemoteAddr() string {
	return c.Addr
}

// Close closes the WebSocket connection with a normal closure status.
func (c *WSConn) Close() error {
	return c.WS.Close(websocket.StatusNormalClosure, "")
}

// WSToNetConn returns a net.Conn adapter for the WebSocket.
// Useful for relay relay pipe where io.Copy needs a standard net.Conn.
func WSToNetConn(ws *websocket.Conn) net.Conn {
	return websocket.NetConn(context.Background(), ws, websocket.MessageBinary)
}

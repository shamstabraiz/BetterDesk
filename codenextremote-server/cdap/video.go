// Package cdap — video handles WebSocket video stream sessions between
// the admin panel and CDAP devices (e.g. IP cameras, surveillance).
// Read-only streams — no input relay, only frame forwarding.
package cdap

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"sync"
	"sync/atomic"
	"time"

	"github.com/coder/websocket"
)

// VideoSession represents an active video stream session relaying
// frames from a CDAP device to the browser.
type VideoSession struct {
	ID       string
	DeviceID string
	Username string
	Role     string

	browser    *websocket.Conn
	deviceConn *DeviceConn

	createdAt time.Time
	mu        sync.Mutex
	closed    atomic.Bool
}

// VideoStartPayload is sent to the device to start a video stream.
type VideoStartPayload struct {
	SessionID  string `json:"session_id"`
	StreamID   string `json:"stream_id,omitempty"` // optional camera/stream selector
	Width      int    `json:"width,omitempty"`
	Height     int    `json:"height,omitempty"`
	Quality    int    `json:"quality,omitempty"` // JPEG quality 1-100
	FPS        int    `json:"fps,omitempty"`
	AudioCodec string `json:"audio_codec,omitempty"` // none, opus, pcm
}

// VideoFramePayload is sent from the device to the browser.
type VideoFramePayload struct {
	SessionID string `json:"session_id"`
	Format    string `json:"format"`    // jpeg, png, h264
	Width     int    `json:"width"`     // frame width
	Height    int    `json:"height"`    // frame height
	Data      string `json:"data"`      // base64-encoded frame
	Timestamp int64  `json:"timestamp"` // capture timestamp ms
	KeyFrame  bool   `json:"key_frame,omitempty"`
}

// VideoEndPayload is sent when a video stream session ends.
type VideoEndPayload struct {
	SessionID string `json:"session_id"`
	Reason    string `json:"reason,omitempty"`
}

// StartVideoSession creates a new video stream session between the
// browser and a CDAP device for live video monitoring.
func (g *Gateway) StartVideoSession(ctx context.Context, browserConn *websocket.Conn, deviceID, username, role string, streamID string, quality, fps int) (*VideoSession, error) {
	dc := g.GetDeviceConn(deviceID)
	if dc == nil {
		return nil, fmt.Errorf("device %s not connected", deviceID)
	}

	if dc.Manifest != nil {
		hasVideo := false
		for _, cap := range dc.Manifest.Capabilities {
			if cap == "video_stream" {
				hasVideo = true
				break
			}
		}
		if !hasVideo {
			return nil, fmt.Errorf("device %s does not support video_stream", deviceID)
		}
	}

	if quality <= 0 || quality > 100 {
		quality = 60
	}
	if fps <= 0 || fps > 30 {
		fps = 10
	}

	sessionID := fmt.Sprintf("vid_%s_%d", deviceID, time.Now().UnixNano())

	vs := &VideoSession{
		ID:         sessionID,
		DeviceID:   deviceID,
		Username:   username,
		Role:       role,
		browser:    browserConn,
		deviceConn: dc,
		createdAt:  time.Now(),
	}

	startPayload := VideoStartPayload{
		SessionID: sessionID,
		StreamID:  streamID,
		Quality:   quality,
		FPS:       fps,
	}
	data, _ := json.Marshal(startPayload)
	msg := &Message{
		Type:      "video_start",
		ID:        sessionID,
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Payload:   data,
	}

	if err := dc.WriteMessage(ctx, msg); err != nil {
		return nil, fmt.Errorf("send video_start to device: %w", err)
	}

	g.videoSessions.Store(sessionID, vs)

	log.Printf("[cdap] Video session %s started for device %s by %s (q%d @%dfps)",
		sessionID, deviceID, username, quality, fps)

	if g.auditLog != nil {
		g.auditLog.Log("cdap_video_started", dc.ClientIP, username, map[string]string{
			"session_id": sessionID,
			"device_id":  deviceID,
		})
	}

	return vs, nil
}

// HandleVideoFrame is called when the device sends a video frame.
func (g *Gateway) HandleVideoFrame(ctx context.Context, sessionID string, frame *VideoFramePayload) error {
	val, ok := g.videoSessions.Load(sessionID)
	if !ok {
		return fmt.Errorf("video session %s not found", sessionID)
	}
	vs := val.(*VideoSession)
	if vs.closed.Load() {
		return nil
	}

	output := map[string]any{
		"type":       "frame",
		"session_id": sessionID,
		"format":     frame.Format,
		"width":      frame.Width,
		"height":     frame.Height,
		"data":       frame.Data,
		"timestamp":  frame.Timestamp,
		"key_frame":  frame.KeyFrame,
	}
	outData, _ := json.Marshal(output)

	vs.mu.Lock()
	defer vs.mu.Unlock()
	return vs.browser.Write(ctx, websocket.MessageText, outData)
}

// EndVideoSession terminates a video stream session.
func (g *Gateway) EndVideoSession(ctx context.Context, sessionID, reason string) {
	val, ok := g.videoSessions.LoadAndDelete(sessionID)
	if !ok {
		return
	}
	vs := val.(*VideoSession)
	if vs.closed.Swap(true) {
		return
	}

	endPayload := VideoEndPayload{
		SessionID: sessionID,
		Reason:    reason,
	}
	data, _ := json.Marshal(endPayload)
	msg := &Message{
		Type:      "video_end",
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Payload:   data,
	}
	vs.deviceConn.WriteMessage(ctx, msg)

	endMsg, _ := json.Marshal(map[string]string{
		"type":       "end",
		"session_id": sessionID,
		"reason":     reason,
	})
	vs.mu.Lock()
	vs.browser.Write(ctx, websocket.MessageText, endMsg)
	vs.mu.Unlock()
	vs.browser.Close(websocket.StatusNormalClosure, reason)

	log.Printf("[cdap] Video session %s ended: %s", sessionID, reason)

	if g.auditLog != nil {
		g.auditLog.Log("cdap_video_ended", vs.deviceConn.ClientIP, vs.Username, map[string]string{
			"session_id": sessionID,
			"device_id":  vs.DeviceID,
			"reason":     reason,
		})
	}
}

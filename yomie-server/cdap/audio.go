// Package cdap — audio handles WebSocket audio stream sessions between
// the admin panel and CDAP devices. Supports bidirectional audio relay
// (device→browser for monitoring, browser→device for communication).
// Audio codec negotiation supports Opus and raw PCM.
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

// AudioSession represents an active audio stream between a browser and device.
type AudioSession struct {
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

// AudioStartPayload is sent to the device to start an audio stream.
type AudioStartPayload struct {
	SessionID  string `json:"session_id"`
	Codec      string `json:"codec"`       // opus, pcm
	SampleRate int    `json:"sample_rate"` // e.g. 48000
	Channels   int    `json:"channels"`    // 1=mono, 2=stereo
	Direction  string `json:"direction"`   // send, receive, bidirectional
}

// AudioFramePayload carries a single audio chunk from device to browser.
type AudioFramePayload struct {
	SessionID string `json:"session_id"`
	Codec     string `json:"codec"`     // opus, pcm
	Data      string `json:"data"`      // base64-encoded audio data
	Timestamp int64  `json:"timestamp"` // capture timestamp ms
	Duration  int    `json:"duration"`  // frame duration ms (typically 20)
	Sequence  int64  `json:"sequence"`  // sequential frame number
}

// AudioEndPayload is sent when an audio session ends.
type AudioEndPayload struct {
	SessionID string `json:"session_id"`
	Reason    string `json:"reason,omitempty"`
}

// StartAudioSession creates a new audio stream session between the
// browser and a CDAP device for audio monitoring or communication.
func (g *Gateway) StartAudioSession(ctx context.Context, browserConn *websocket.Conn, deviceID, username, role string, codec string, sampleRate, channels int, direction string) (*AudioSession, error) {
	dc := g.GetDeviceConn(deviceID)
	if dc == nil {
		return nil, fmt.Errorf("device %s not connected", deviceID)
	}

	// Check audio capability
	if dc.Manifest != nil {
		hasAudio := false
		for _, cap := range dc.Manifest.Capabilities {
			if cap == "audio" {
				hasAudio = true
				break
			}
		}
		if !hasAudio {
			return nil, fmt.Errorf("device %s does not support audio", deviceID)
		}
	}

	// Defaults
	if codec == "" {
		codec = "opus"
	}
	if sampleRate <= 0 {
		sampleRate = 48000
	}
	if channels <= 0 || channels > 2 {
		channels = 1
	}
	if direction == "" {
		direction = "receive"
	}

	sessionID := fmt.Sprintf("aud_%s_%d", deviceID, time.Now().UnixNano())

	as := &AudioSession{
		ID:         sessionID,
		DeviceID:   deviceID,
		Username:   username,
		Role:       role,
		browser:    browserConn,
		deviceConn: dc,
		createdAt:  time.Now(),
	}

	startPayload := AudioStartPayload{
		SessionID:  sessionID,
		Codec:      codec,
		SampleRate: sampleRate,
		Channels:   channels,
		Direction:  direction,
	}
	data, _ := json.Marshal(startPayload)
	msg := &Message{
		Type:      "audio_start",
		ID:        sessionID,
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Payload:   data,
	}

	if err := dc.WriteMessage(ctx, msg); err != nil {
		return nil, fmt.Errorf("send audio_start to device: %w", err)
	}

	g.audioSessions.Store(sessionID, as)

	log.Printf("[cdap] Audio session %s started for device %s by %s (codec=%s rate=%d ch=%d dir=%s)",
		sessionID, deviceID, username, codec, sampleRate, channels, direction)

	if g.auditLog != nil {
		g.auditLog.Log("cdap_audio_started", dc.ClientIP, username, map[string]string{
			"session_id": sessionID,
			"device_id":  deviceID,
			"codec":      codec,
			"direction":  direction,
		})
	}

	return as, nil
}

// HandleAudioFrame is called when the device sends an audio frame.
func (g *Gateway) HandleAudioFrame(ctx context.Context, sessionID string, frame *AudioFramePayload) error {
	val, ok := g.audioSessions.Load(sessionID)
	if !ok {
		return fmt.Errorf("audio session %s not found", sessionID)
	}
	as := val.(*AudioSession)
	if as.closed.Load() {
		return nil
	}

	output := map[string]any{
		"type":       "audio_frame",
		"session_id": sessionID,
		"codec":      frame.Codec,
		"data":       frame.Data,
		"timestamp":  frame.Timestamp,
		"duration":   frame.Duration,
		"sequence":   frame.Sequence,
	}
	outData, _ := json.Marshal(output)

	as.mu.Lock()
	defer as.mu.Unlock()
	return as.browser.Write(ctx, websocket.MessageText, outData)
}

// RelayAudioInput forwards audio data from the browser to the device
// (for bidirectional audio sessions, e.g. intercom / voice communication).
func (g *Gateway) RelayAudioInput(ctx context.Context, sessionID string, codec, data string, timestamp int64) error {
	val, ok := g.audioSessions.Load(sessionID)
	if !ok {
		return fmt.Errorf("audio session %s not found", sessionID)
	}
	as := val.(*AudioSession)
	if as.closed.Load() {
		return fmt.Errorf("audio session %s is closed", sessionID)
	}

	payload := map[string]any{
		"session_id": sessionID,
		"codec":      codec,
		"data":       data,
		"timestamp":  timestamp,
	}
	payloadData, _ := json.Marshal(payload)
	msg := &Message{
		Type:      "audio_input",
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Payload:   payloadData,
	}

	return as.deviceConn.WriteMessage(ctx, msg)
}

// EndAudioSession terminates an audio session.
func (g *Gateway) EndAudioSession(ctx context.Context, sessionID, reason string) {
	val, ok := g.audioSessions.LoadAndDelete(sessionID)
	if !ok {
		return
	}
	as := val.(*AudioSession)
	if as.closed.Swap(true) {
		return
	}

	endPayload := AudioEndPayload{
		SessionID: sessionID,
		Reason:    reason,
	}
	data, _ := json.Marshal(endPayload)
	msg := &Message{
		Type:      "audio_end",
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Payload:   data,
	}
	as.deviceConn.WriteMessage(ctx, msg)

	endMsg, _ := json.Marshal(map[string]string{
		"type":       "end",
		"session_id": sessionID,
		"reason":     reason,
	})
	as.mu.Lock()
	as.browser.Write(ctx, websocket.MessageText, endMsg)
	as.mu.Unlock()
	as.browser.Close(websocket.StatusNormalClosure, reason)

	log.Printf("[cdap] Audio session %s ended: %s", sessionID, reason)

	if g.auditLog != nil {
		g.auditLog.Log("cdap_audio_ended", as.deviceConn.ClientIP, as.Username, map[string]string{
			"session_id": sessionID,
			"device_id":  as.DeviceID,
			"reason":     reason,
		})
	}
}

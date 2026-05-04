// Package cdap — clipboard handles bidirectional clipboard synchronization
// between the admin panel and CDAP devices. Supports text and image data.
package cdap

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"time"

	"github.com/coder/websocket"
)

// ClipboardPayload represents clipboard data exchanged between viewer and device.
type ClipboardPayload struct {
	Type      string `json:"type"`       // "clipboard_set" or "clipboard_update"
	Format    string `json:"format"`     // "text", "image", "html"
	Data      string `json:"data"`       // text content or base64-encoded binary
	SessionID string `json:"session_id"` // associated desktop session
}

// RelayClipboard forwards clipboard data from the viewer to the CDAP device.
func (gw *Gateway) RelayClipboard(ctx context.Context, deviceID, sessionID, format, data string) error {
	val, ok := gw.devices.Load(deviceID)
	if !ok {
		return fmt.Errorf("device %s not connected", deviceID)
	}
	dc := val.(*DeviceConn)

	msg := &Message{
		Type:      "clipboard_set",
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Payload: mustMarshal(map[string]string{
			"session_id": sessionID,
			"format":     format,
			"data":       data,
		}),
	}
	return dc.WriteMessage(ctx, msg)
}

// HandleClipboardUpdate processes clipboard data from a device and forwards
// it to the active desktop session viewer (if any).
func (gw *Gateway) HandleClipboardUpdate(deviceID string, payload json.RawMessage) {
	var clip ClipboardPayload
	if err := json.Unmarshal(payload, &clip); err != nil {
		log.Printf("[cdap] Invalid clipboard payload from %s: %v", deviceID, err)
		return
	}

	// Validate format
	switch clip.Format {
	case "text", "image", "html":
		// valid
	default:
		log.Printf("[cdap] Unknown clipboard format from %s: %s", deviceID, clip.Format)
		return
	}

	// Limit clipboard data size (8MB max for images)
	const maxClipboardSize = 8 * 1024 * 1024
	if len(clip.Data) > maxClipboardSize {
		log.Printf("[cdap] Clipboard data too large from %s: %d bytes", deviceID, len(clip.Data))
		return
	}

	// Find active desktop session and forward clipboard to viewer
	if clip.SessionID != "" {
		if sessionVal, ok := gw.desktopSessions.Load(clip.SessionID); ok {
			session := sessionVal.(*DesktopSession)
			if session.DeviceID == deviceID {
				fwdMsg, _ := json.Marshal(map[string]string{
					"type":       "clipboard_update",
					"format":     clip.Format,
					"data":       clip.Data,
					"session_id": clip.SessionID,
				})
				session.mu.Lock()
				_ = session.browser.Write(context.Background(), websocket.MessageText, fwdMsg)
				session.mu.Unlock()
			}
		}
	}

	gw.auditAction("clipboard_sync", deviceID, map[string]string{
		"format": clip.Format,
		"size":   fmt.Sprintf("%d", len(clip.Data)),
	})
}

// mustMarshal marshals v to JSON, panicking on error (for static structures).
func mustMarshal(v any) json.RawMessage {
	data, err := json.Marshal(v)
	if err != nil {
		panic(fmt.Sprintf("mustMarshal: %v", err))
	}
	return data
}

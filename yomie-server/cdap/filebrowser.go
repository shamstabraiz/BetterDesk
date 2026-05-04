// Package cdap — filebrowser handles file browsing and transfer sessions
// between the admin panel and CDAP devices. Uses a request-response pattern
// over WebSocket for directory listing, file download, and file upload.
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

// FileSession represents an active file browser session.
type FileSession struct {
	ID       string
	DeviceID string
	Username string
	Role     string

	browser    *websocket.Conn
	deviceConn *DeviceConn

	// pending tracks in-flight requests awaiting device response.
	pending sync.Map // requestID → chan *Message

	createdAt time.Time
	mu        sync.Mutex
	closed    atomic.Bool
}

// FileListRequest asks the device to list a directory.
type FileListRequest struct {
	SessionID string `json:"session_id"`
	RequestID string `json:"request_id"`
	Path      string `json:"path"`
}

// FileListResponse is sent from the device with directory contents.
type FileListResponse struct {
	SessionID string      `json:"session_id"`
	RequestID string      `json:"request_id"`
	Path      string      `json:"path"`
	Entries   []FileEntry `json:"entries"`
	Error     string      `json:"error,omitempty"`
}

// FileEntry represents a single file or directory.
type FileEntry struct {
	Name     string `json:"name"`
	IsDir    bool   `json:"is_dir"`
	Size     int64  `json:"size"`
	Modified string `json:"modified"` // RFC3339
	Mode     string `json:"mode,omitempty"`
}

// FileReadRequest asks the device to read (download) a file.
type FileReadRequest struct {
	SessionID string `json:"session_id"`
	RequestID string `json:"request_id"`
	Path      string `json:"path"`
	Offset    int64  `json:"offset,omitempty"`
	Length    int64  `json:"length,omitempty"` // 0 = entire file
}

// FileReadResponse is sent from the device with file data.
type FileReadResponse struct {
	SessionID string `json:"session_id"`
	RequestID string `json:"request_id"`
	Path      string `json:"path"`
	Data      string `json:"data"` // base64-encoded content
	Size      int64  `json:"size"` // total file size
	Offset    int64  `json:"offset"`
	Done      bool   `json:"done"`
	Error     string `json:"error,omitempty"`
}

// FileWriteRequest asks the device to create/write a file.
type FileWriteRequest struct {
	SessionID string `json:"session_id"`
	RequestID string `json:"request_id"`
	Path      string `json:"path"`
	Data      string `json:"data"` // base64-encoded content
	Offset    int64  `json:"offset,omitempty"`
	Done      bool   `json:"done"` // true = last chunk
}

// FileWriteResponse confirms write status.
type FileWriteResponse struct {
	SessionID string `json:"session_id"`
	RequestID string `json:"request_id"`
	Path      string `json:"path"`
	Written   int64  `json:"written"`
	Error     string `json:"error,omitempty"`
}

// FileDeleteRequest asks the device to delete a file or directory.
type FileDeleteRequest struct {
	SessionID string `json:"session_id"`
	RequestID string `json:"request_id"`
	Path      string `json:"path"`
}

// FileEndPayload is sent when a file browser session ends.
type FileEndPayload struct {
	SessionID string `json:"session_id"`
	Reason    string `json:"reason,omitempty"`
}

// StartFileSession creates a new file browser session between the
// browser and a CDAP device.
func (g *Gateway) StartFileSession(ctx context.Context, browserConn *websocket.Conn, deviceID, username, role string) (*FileSession, error) {
	dc := g.GetDeviceConn(deviceID)
	if dc == nil {
		return nil, fmt.Errorf("device %s not connected", deviceID)
	}

	if dc.Manifest != nil {
		hasFile := false
		for _, cap := range dc.Manifest.Capabilities {
			if cap == "file_transfer" {
				hasFile = true
				break
			}
		}
		if !hasFile {
			return nil, fmt.Errorf("device %s does not support file_transfer", deviceID)
		}
	}

	sessionID := fmt.Sprintf("file_%s_%d", deviceID, time.Now().UnixNano())

	fs := &FileSession{
		ID:         sessionID,
		DeviceID:   deviceID,
		Username:   username,
		Role:       role,
		browser:    browserConn,
		deviceConn: dc,
		createdAt:  time.Now(),
	}

	// Notify device that a file browser session is starting.
	startPayload, _ := json.Marshal(map[string]string{
		"session_id": sessionID,
	})
	msg := &Message{
		Type:      "file_start",
		ID:        sessionID,
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Payload:   startPayload,
	}

	if err := dc.WriteMessage(ctx, msg); err != nil {
		return nil, fmt.Errorf("send file_start to device: %w", err)
	}

	g.fileSessions.Store(sessionID, fs)

	log.Printf("[cdap] File session %s started for device %s by %s", sessionID, deviceID, username)

	if g.auditLog != nil {
		g.auditLog.Log("cdap_file_started", dc.ClientIP, username, map[string]string{
			"session_id": sessionID,
			"device_id":  deviceID,
		})
	}

	return fs, nil
}

// RelayFileRequest forwards a file browser request (list, read, write, delete)
// from the browser to the device.
func (g *Gateway) RelayFileRequest(ctx context.Context, sessionID, requestType string, payload json.RawMessage) error {
	val, ok := g.fileSessions.Load(sessionID)
	if !ok {
		return fmt.Errorf("file session %s not found", sessionID)
	}
	fs := val.(*FileSession)
	if fs.closed.Load() {
		return fmt.Errorf("file session %s is closed", sessionID)
	}

	msg := &Message{
		Type:      requestType, // file_list, file_read, file_write, file_delete
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Payload:   payload,
	}

	return fs.deviceConn.WriteMessage(ctx, msg)
}

// HandleFileResponse is called when the device sends a file browser response.
// It forwards the response to the browser WebSocket.
func (g *Gateway) HandleFileResponse(ctx context.Context, sessionID, responseType string, payload json.RawMessage) error {
	val, ok := g.fileSessions.Load(sessionID)
	if !ok {
		return fmt.Errorf("file session %s not found", sessionID)
	}
	fs := val.(*FileSession)
	if fs.closed.Load() {
		return nil
	}

	output := map[string]any{
		"type":       responseType,
		"session_id": sessionID,
	}

	// Parse the payload to embed it inline
	var raw map[string]any
	if json.Unmarshal(payload, &raw) == nil {
		for k, v := range raw {
			output[k] = v
		}
	}

	outData, _ := json.Marshal(output)

	fs.mu.Lock()
	defer fs.mu.Unlock()
	return fs.browser.Write(ctx, websocket.MessageText, outData)
}

// EndFileSession terminates a file browser session.
func (g *Gateway) EndFileSession(ctx context.Context, sessionID, reason string) {
	val, ok := g.fileSessions.LoadAndDelete(sessionID)
	if !ok {
		return
	}
	fs := val.(*FileSession)
	if fs.closed.Swap(true) {
		return
	}

	endPayload := FileEndPayload{
		SessionID: sessionID,
		Reason:    reason,
	}
	data, _ := json.Marshal(endPayload)
	msg := &Message{
		Type:      "file_end",
		Timestamp: time.Now().UTC().Format(time.RFC3339),
		Payload:   data,
	}
	fs.deviceConn.WriteMessage(ctx, msg)

	endMsg, _ := json.Marshal(map[string]string{
		"type":       "end",
		"session_id": sessionID,
		"reason":     reason,
	})
	fs.mu.Lock()
	fs.browser.Write(ctx, websocket.MessageText, endMsg)
	fs.mu.Unlock()
	fs.browser.Close(websocket.StatusNormalClosure, reason)

	log.Printf("[cdap] File session %s ended: %s", sessionID, reason)

	if g.auditLog != nil {
		g.auditLog.Log("cdap_file_ended", fs.deviceConn.ClientIP, fs.Username, map[string]string{
			"session_id": sessionID,
			"device_id":  fs.DeviceID,
			"reason":     reason,
		})
	}
}

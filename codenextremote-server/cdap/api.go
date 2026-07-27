// Package cdap provides REST-friendly accessors for the CDAP gateway state.
// These methods are consumed by the API server to serve panel requests.
package cdap

import (
	"context"
	"encoding/json"
	"fmt"
	"time"
)

// DeviceInfo is a REST-friendly snapshot of a CDAP device's state.
type DeviceInfo struct {
	ID                string          `json:"id"`
	Connected         bool            `json:"connected"`
	Manifest          json.RawMessage `json:"manifest,omitempty"`
	WidgetState       map[string]any  `json:"widget_state,omitempty"`
	ConnectedAt       *time.Time      `json:"connected_at,omitempty"`
	LastHeartbeat     *time.Time      `json:"last_heartbeat,omitempty"`
	SessionID         string          `json:"session_id,omitempty"`
	HeartbeatCount    int64           `json:"heartbeat_count"`
	CommandCount      int64           `json:"command_count"`
	HeartbeatInterval int             `json:"heartbeat_interval"`
	Username          string          `json:"username,omitempty"`
	Role              string          `json:"role,omitempty"`
	ClientIP          string          `json:"client_ip,omitempty"`
}

// GetDeviceInfo returns a full snapshot of a connected CDAP device.
// Returns nil if the device is not connected via CDAP.
func (g *Gateway) GetDeviceInfo(id string) *DeviceInfo {
	dc := g.GetDeviceConn(id)
	if dc == nil {
		return nil
	}

	info := &DeviceInfo{
		ID:                dc.ID,
		Connected:         true,
		SessionID:         dc.SessionID,
		HeartbeatCount:    dc.HeartbeatCount.Load(),
		CommandCount:      dc.CommandCount.Load(),
		HeartbeatInterval: dc.HeartbeatInterval,
		Username:          dc.Username,
		Role:              dc.Role,
		ClientIP:          dc.ClientIP,
	}

	connAt := dc.ConnectedAt
	info.ConnectedAt = &connAt
	lastHB := dc.LastHeartbeat
	if !lastHB.IsZero() {
		info.LastHeartbeat = &lastHB
	}

	if dc.Manifest != nil {
		data, err := json.Marshal(dc.Manifest)
		if err == nil {
			info.Manifest = data
		}
	}

	// Collect widget state
	state := make(map[string]any)
	dc.widgetState.Range(func(key, value any) bool {
		state[key.(string)] = value
		return true
	})
	if len(state) > 0 {
		info.WidgetState = state
	}

	return info
}

// GetDeviceManifest returns the manifest JSON for a device.
// Checks in-memory first (connected device), then falls back to DB.
func (g *Gateway) GetDeviceManifest(id string) (json.RawMessage, bool) {
	// In-memory (connected device)
	if dc := g.GetDeviceConn(id); dc != nil && dc.Manifest != nil {
		data, err := json.Marshal(dc.Manifest)
		if err == nil {
			return data, true
		}
	}
	// Fall back to DB (stored on registration)
	val, err := g.db.GetConfig("cdap_manifest_" + id)
	if err != nil || val == "" {
		return nil, false
	}
	return json.RawMessage(val), true
}

// GetDeviceWidgetState returns current widget values for a connected device.
func (g *Gateway) GetDeviceWidgetState(id string) (map[string]any, bool) {
	dc := g.GetDeviceConn(id)
	if dc == nil {
		return nil, false
	}
	state := make(map[string]any)
	dc.widgetState.Range(func(key, value any) bool {
		state[key.(string)] = value
		return true
	})
	return state, true
}

// IsConnected returns true if the device has an active CDAP connection.
func (g *Gateway) IsConnected(id string) bool {
	return g.GetDeviceConn(id) != nil
}

// GetWidget looks up a widget by ID from a connected device's manifest.
// Returns nil if the device is not connected or widget not found.
func (g *Gateway) GetWidget(deviceID, widgetID string) *Widget {
	dc := g.GetDeviceConn(deviceID)
	if dc == nil || dc.Manifest == nil {
		return nil
	}
	for i := range dc.Manifest.Widgets {
		if dc.Manifest.Widgets[i].ID == widgetID {
			return &dc.Manifest.Widgets[i]
		}
	}
	return nil
}

// SendCommandJSON builds and sends a command to a connected CDAP device.
// The caller must perform RBAC checks before invoking this method.
func (g *Gateway) SendCommandJSON(ctx context.Context, deviceID, commandID, widgetID, action string, value any, operator, reason string) error {
	payload := CommandPayload{
		CommandID: commandID,
		WidgetID:  widgetID,
		Action:    action,
		Value:     value,
		Operator:  operator,
		Reason:    reason,
	}

	data, err := json.Marshal(payload)
	if err != nil {
		return fmt.Errorf("marshal command: %w", err)
	}

	cmd := &CommandMessage{
		ID:      commandID,
		Payload: data,
	}

	if err := g.SendCommand(ctx, deviceID, cmd); err != nil {
		return err
	}

	g.auditAction("cdap_command_sent", deviceID, map[string]string{
		"command_id": commandID,
		"widget_id":  widgetID,
		"action":     action,
		"operator":   operator,
	})

	return nil
}

// ListConnectedDevices returns IDs of all connected CDAP devices.
func (g *Gateway) ListConnectedDevices() []string {
	var ids []string
	g.devices.Range(func(key, value any) bool {
		ids = append(ids, key.(string))
		return true
	})
	return ids
}

// GetActiveAlerts returns all currently firing CDAP alerts.
// If deviceID is non-empty, only alerts for that device are returned.
func (g *Gateway) GetActiveAlerts(deviceID string) []*AlertState {
	if g.alertEngine == nil {
		return nil
	}
	return g.alertEngine.GetActiveAlerts(deviceID)
}

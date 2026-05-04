package cdap

import (
	"encoding/json"
	"fmt"
	"log"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/unitronix/betterdesk-server/events"
)

// AlertState tracks whether an individual alert is currently firing.
type AlertState struct {
	AlertID   string    `json:"alert_id"`
	DeviceID  string    `json:"device_id"`
	Label     string    `json:"label"`
	Severity  string    `json:"severity"` // critical, warning, info
	Message   string    `json:"message"`
	Firing    bool      `json:"firing"`
	FiredAt   time.Time `json:"fired_at,omitempty"`
	ClearedAt time.Time `json:"cleared_at,omitempty"`
}

// AlertEngine evaluates AlertDef conditions against widget state and
// publishes events when alerts fire or clear.
type AlertEngine struct {
	eventBus *events.Bus
	mu       sync.RWMutex
	// active: deviceID → alertID → AlertState
	active map[string]map[string]*AlertState
}

// NewAlertEngine creates a new alert processing engine.
func NewAlertEngine(bus *events.Bus) *AlertEngine {
	return &AlertEngine{
		eventBus: bus,
		active:   make(map[string]map[string]*AlertState),
	}
}

// Evaluate checks all alert definitions from the manifest against the
// current widget state and fires/clears alerts as needed.
func (ae *AlertEngine) Evaluate(deviceID string, manifest *Manifest, widgetState map[string]any) {
	if manifest == nil || len(manifest.Alerts) == 0 {
		return
	}

	ae.mu.Lock()
	defer ae.mu.Unlock()

	if ae.active[deviceID] == nil {
		ae.active[deviceID] = make(map[string]*AlertState)
	}
	deviceAlerts := ae.active[deviceID]

	for i := range manifest.Alerts {
		alert := &manifest.Alerts[i]
		firing := evaluateCondition(alert.Condition, widgetState)
		existing := deviceAlerts[alert.ID]

		if firing && (existing == nil || !existing.Firing) {
			// Alert just fired
			state := &AlertState{
				AlertID:  alert.ID,
				DeviceID: deviceID,
				Label:    alert.Label,
				Severity: alert.Severity,
				Message:  interpolateMessage(alert.Message, widgetState),
				Firing:   true,
				FiredAt:  time.Now(),
			}
			deviceAlerts[alert.ID] = state

			log.Printf("[cdap] %s: alert FIRED %s (%s): %s",
				deviceID, alert.ID, alert.Severity, state.Message)

			ae.publishAlertEvent(state, "cdap_alert_fired")

		} else if !firing && existing != nil && existing.Firing {
			// Alert cleared
			existing.Firing = false
			existing.ClearedAt = time.Now()

			log.Printf("[cdap] %s: alert CLEARED %s", deviceID, alert.ID)

			ae.publishAlertEvent(existing, "cdap_alert_cleared")
		}
	}
}

// RemoveDevice cleans up alert state when a device disconnects.
func (ae *AlertEngine) RemoveDevice(deviceID string) {
	ae.mu.Lock()
	defer ae.mu.Unlock()

	if alerts, ok := ae.active[deviceID]; ok {
		for _, state := range alerts {
			if state.Firing {
				state.Firing = false
				state.ClearedAt = time.Now()
				ae.publishAlertEvent(state, "cdap_alert_cleared")
			}
		}
		delete(ae.active, deviceID)
	}
}

// GetActiveAlerts returns all currently firing alerts, optionally for a single device.
func (ae *AlertEngine) GetActiveAlerts(deviceID string) []*AlertState {
	ae.mu.RLock()
	defer ae.mu.RUnlock()

	var result []*AlertState
	if deviceID != "" {
		if alerts, ok := ae.active[deviceID]; ok {
			for _, a := range alerts {
				if a.Firing {
					result = append(result, a)
				}
			}
		}
	} else {
		for _, alerts := range ae.active {
			for _, a := range alerts {
				if a.Firing {
					result = append(result, a)
				}
			}
		}
	}
	return result
}

func (ae *AlertEngine) publishAlertEvent(state *AlertState, eventType string) {
	if ae.eventBus == nil {
		return
	}
	ae.eventBus.Publish(events.Event{
		Type: events.EventType(eventType),
		Data: map[string]string{
			"peer_id":  state.DeviceID,
			"alert_id": state.AlertID,
			"label":    state.Label,
			"severity": state.Severity,
			"message":  state.Message,
			"firing":   fmt.Sprintf("%t", state.Firing),
		},
	})
}

// ── Condition Evaluator ──────────────────────────────────────────────

// condRegexp matches simple conditions like:
//
//	"temperature > 80"
//	"cpu >= 95.5"
//	"status == 'error'"
//	"relay_active != true"
var condRegexp = regexp.MustCompile(`^([a-zA-Z_][a-zA-Z0-9_.]*)\s*(==|!=|>=|<=|>|<)\s*(.+)$`)

// evaluateCondition evaluates a simple expression against widget state.
// Supported formats:
//
//	"widget_id > 80"     (numeric comparison)
//	"widget_id == true"  (boolean)
//	"widget_id == 'on'"  (string)
//	"widget_id != 0"     (numeric)
//
// Returns false for unparseable conditions or missing widget values (fail-safe).
func evaluateCondition(condition string, state map[string]any) bool {
	condition = strings.TrimSpace(condition)
	if condition == "" {
		return false
	}

	matches := condRegexp.FindStringSubmatch(condition)
	if matches == nil {
		return false
	}

	widgetID := matches[1]
	operator := matches[2]
	expected := strings.TrimSpace(matches[3])

	actual, ok := state[widgetID]
	if !ok {
		return false // widget value not available → not firing
	}

	return compare(actual, operator, expected)
}

func compare(actual any, op string, expected string) bool {
	// Try numeric comparison
	actualNum, actualIsNum := toFloat64(actual)
	expectedNum, expectedIsNum := toFloat64Str(expected)

	if actualIsNum && expectedIsNum {
		switch op {
		case "==":
			return actualNum == expectedNum
		case "!=":
			return actualNum != expectedNum
		case ">":
			return actualNum > expectedNum
		case "<":
			return actualNum < expectedNum
		case ">=":
			return actualNum >= expectedNum
		case "<=":
			return actualNum <= expectedNum
		}
		return false
	}

	// String/bool comparison
	actualStr := fmt.Sprintf("%v", actual)
	// Strip quotes from expected
	expected = strings.Trim(expected, "'\"")

	switch op {
	case "==":
		return strings.EqualFold(actualStr, expected)
	case "!=":
		return !strings.EqualFold(actualStr, expected)
	default:
		return false
	}
}

func toFloat64(v any) (float64, bool) {
	switch n := v.(type) {
	case float64:
		return n, true
	case float32:
		return float64(n), true
	case int:
		return float64(n), true
	case int64:
		return float64(n), true
	case json.Number:
		f, err := n.Float64()
		return f, err == nil
	case bool:
		if n {
			return 1, true
		}
		return 0, true
	case string:
		f, err := strconv.ParseFloat(n, 64)
		return f, err == nil
	default:
		return 0, false
	}
}

func toFloat64Str(s string) (float64, bool) {
	s = strings.Trim(s, "'\"")
	// Handle boolean keywords
	switch strings.ToLower(s) {
	case "true":
		return 1, true
	case "false":
		return 0, true
	}
	f, err := strconv.ParseFloat(s, 64)
	return f, err == nil
}

// interpolateMessage substitutes {widget_id} placeholders in the message
// with actual widget values.
func interpolateMessage(msg string, state map[string]any) string {
	return regexp.MustCompile(`\{([a-zA-Z_][a-zA-Z0-9_.]*)\}`).ReplaceAllStringFunc(msg, func(match string) string {
		key := match[1 : len(match)-1]
		if val, ok := state[key]; ok {
			return fmt.Sprintf("%v", val)
		}
		return match
	})
}

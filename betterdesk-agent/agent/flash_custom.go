package agent

import (
	"encoding/json"
	"log"
)

// handleToggleFlashCustom receives CDAP toggle_flash_custom from the console (e.g. web remote).
// Extend with platform torch / flashlight APIs as needed.
func (a *Agent) handleToggleFlashCustom(msg *Message) {
	var p struct {
		SessionID string `json:"session_id"`
	}
	_ = json.Unmarshal(msg.Payload, &p)
	log.Printf("[agent] toggle_flash_custom received (session_id=%s) — hook OS torch here", p.SessionID)
}

// Package cdap — auth delegation allows admins to grant time-limited
// elevated access to specific devices and widgets for other users.
package cdap

import (
	"fmt"
	"sync"
	"time"
)

// Delegation represents a temporary privilege grant from an admin to
// another user for specific device(s) and optional widget(s).
type Delegation struct {
	ID        string    `json:"id"`
	Grantor   string    `json:"grantor"`    // admin who created this delegation
	Grantee   string    `json:"grantee"`    // target user receiving elevated access
	DeviceID  string    `json:"device_id"`  // CDAP device ID (required)
	WidgetIDs []string  `json:"widget_ids"` // empty = all widgets on that device
	Role      string    `json:"role"`       // elevated role to grant (operator, admin)
	ExpiresAt time.Time `json:"expires_at"`
	CreatedAt time.Time `json:"created_at"`
	Reason    string    `json:"reason,omitempty"`
}

// DelegationStore manages active delegations in memory.
// Delegations are ephemeral and are lost on server restart.
type DelegationStore struct {
	mu          sync.RWMutex
	delegations map[string]*Delegation // keyed by ID
	byGrantee   map[string][]*Delegation
}

// NewDelegationStore creates a new delegation store.
func NewDelegationStore() *DelegationStore {
	return &DelegationStore{
		delegations: make(map[string]*Delegation),
		byGrantee:   make(map[string][]*Delegation),
	}
}

// Add stores a new delegation. Returns error if ID already exists.
func (ds *DelegationStore) Add(d *Delegation) error {
	ds.mu.Lock()
	defer ds.mu.Unlock()

	if _, exists := ds.delegations[d.ID]; exists {
		return fmt.Errorf("delegation %s already exists", d.ID)
	}

	ds.delegations[d.ID] = d
	ds.byGrantee[d.Grantee] = append(ds.byGrantee[d.Grantee], d)
	return nil
}

// Revoke removes a delegation by ID. Returns true if it existed.
func (ds *DelegationStore) Revoke(id string) bool {
	ds.mu.Lock()
	defer ds.mu.Unlock()

	d, ok := ds.delegations[id]
	if !ok {
		return false
	}
	delete(ds.delegations, id)

	// Remove from grantee index
	list := ds.byGrantee[d.Grantee]
	for i, dd := range list {
		if dd.ID == id {
			ds.byGrantee[d.Grantee] = append(list[:i], list[i+1:]...)
			break
		}
	}
	if len(ds.byGrantee[d.Grantee]) == 0 {
		delete(ds.byGrantee, d.Grantee)
	}
	return true
}

// GetEffectiveRole returns the highest role the user has for a specific device
// and widget via active (non-expired) delegations. Returns "" if no delegation
// applies.
func (ds *DelegationStore) GetEffectiveRole(username, deviceID, widgetID string) string {
	ds.mu.RLock()
	defer ds.mu.RUnlock()

	now := time.Now()
	bestLevel := 0

	for _, d := range ds.byGrantee[username] {
		if d.ExpiresAt.Before(now) {
			continue
		}
		if d.DeviceID != deviceID {
			continue
		}
		// Check if this delegation covers the specific widget
		if len(d.WidgetIDs) > 0 && widgetID != "" {
			found := false
			for _, wid := range d.WidgetIDs {
				if wid == widgetID {
					found = true
					break
				}
			}
			if !found {
				continue
			}
		}
		level := roleLevel[d.Role]
		if level > bestLevel {
			bestLevel = level
		}
	}

	// Map back to role name
	for role, level := range roleLevel {
		if level == bestLevel {
			return role
		}
	}
	return ""
}

// ListByGrantee returns all active (non-expired) delegations for a user.
func (ds *DelegationStore) ListByGrantee(username string) []*Delegation {
	ds.mu.RLock()
	defer ds.mu.RUnlock()

	now := time.Now()
	var result []*Delegation
	for _, d := range ds.byGrantee[username] {
		if d.ExpiresAt.After(now) {
			result = append(result, d)
		}
	}
	return result
}

// ListAll returns all active (non-expired) delegations.
func (ds *DelegationStore) ListAll() []*Delegation {
	ds.mu.RLock()
	defer ds.mu.RUnlock()

	now := time.Now()
	result := make([]*Delegation, 0, len(ds.delegations))
	for _, d := range ds.delegations {
		if d.ExpiresAt.After(now) {
			result = append(result, d)
		}
	}
	return result
}

// CleanExpired removes all expired delegations.
func (ds *DelegationStore) CleanExpired() int {
	ds.mu.Lock()
	defer ds.mu.Unlock()

	now := time.Now()
	count := 0
	for id, d := range ds.delegations {
		if d.ExpiresAt.Before(now) {
			delete(ds.delegations, id)
			count++
		}
	}
	// Rebuild grantee index
	if count > 0 {
		ds.byGrantee = make(map[string][]*Delegation)
		for _, d := range ds.delegations {
			ds.byGrantee[d.Grantee] = append(ds.byGrantee[d.Grantee], d)
		}
	}
	return count
}

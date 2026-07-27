// Package reload provides graceful configuration reload support.
// On Unix systems, it listens for SIGHUP to trigger reload.
// On Windows, a manual reload can be triggered via the admin TCP interface.
package reload

import (
	"log"
	"sync"
)

// Handler manages reload callbacks.
type Handler struct {
	mu       sync.Mutex
	callbacks []func() error
}

// NewHandler creates a new reload handler.
func NewHandler() *Handler {
	return &Handler{}
}

// OnReload registers a callback to be invoked on config reload.
// Callbacks are executed in registration order.
func (h *Handler) OnReload(fn func() error) {
	h.mu.Lock()
	defer h.mu.Unlock()
	h.callbacks = append(h.callbacks, fn)
}

// Execute runs all registered reload callbacks.
// Returns the first error encountered, but continues executing all callbacks.
func (h *Handler) Execute() error {
	h.mu.Lock()
	cbs := make([]func() error, len(h.callbacks))
	copy(cbs, h.callbacks)
	h.mu.Unlock()

	log.Printf("[reload] Executing %d reload callbacks...", len(cbs))
	var firstErr error
	for i, cb := range cbs {
		if err := cb(); err != nil {
			log.Printf("[reload] Callback %d failed: %v", i, err)
			if firstErr == nil {
				firstErr = err
			}
		}
	}
	if firstErr == nil {
		log.Printf("[reload] All callbacks completed successfully")
	}
	return firstErr
}

//go:build !windows

package reload

import (
	"log"
	"os"
	"os/signal"
	"syscall"
)

// ListenSIGHUP starts listening for SIGHUP signals on Unix systems.
// When SIGHUP is received, all registered reload callbacks are executed.
// This function blocks until the context signals are done — call in a goroutine.
func (h *Handler) ListenSIGHUP(done <-chan struct{}) {
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGHUP)
	defer signal.Stop(sigCh)

	log.Printf("[reload] Listening for SIGHUP (config reload)")
	for {
		select {
		case <-done:
			return
		case <-sigCh:
			log.Printf("[reload] Received SIGHUP, reloading configuration...")
			h.Execute()
		}
	}
}

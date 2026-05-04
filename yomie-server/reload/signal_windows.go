//go:build windows

package reload

import (
	"log"
)

// ListenSIGHUP is a no-op on Windows (SIGHUP is not available).
// Use the admin TCP interface "reload" command instead.
func (h *Handler) ListenSIGHUP(done <-chan struct{}) {
	log.Printf("[reload] SIGHUP not available on Windows; use admin TCP 'reload' command")
	<-done
}

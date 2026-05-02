// Package ratelimit implements IP-based rate limiting for the BetterDesk server.
// It uses a sliding window approach: each IP tracks a list of timestamps.
// If the number of events in the window exceeds the limit, new events are rejected.
package ratelimit

import (
	"sync"
	"time"
)

// IPLimiter tracks registration attempts per IP address using a sliding window.
type IPLimiter struct {
	mu      sync.Mutex
	entries map[string]*ipEntry
	limit   int           // Max events per window
	window  time.Duration // Sliding window duration
	cleanup time.Duration // How often to clean stale entries
	stopCh  chan struct{}
	stopped bool
}

type ipEntry struct {
	timestamps []time.Time
	blocked    bool      // Temporarily blocked (too many attempts)
	blockedAt  time.Time // When the block was applied
}

// NewIPLimiter creates a new IP rate limiter.
//
// Parameters:
//   - limit: maximum number of events allowed per window
//   - window: sliding window duration
//   - cleanup: how often stale entries are removed
func NewIPLimiter(limit int, window, cleanup time.Duration) *IPLimiter {
	l := &IPLimiter{
		entries: make(map[string]*ipEntry),
		limit:   limit,
		window:  window,
		cleanup: cleanup,
		stopCh:  make(chan struct{}),
	}
	go l.cleanupLoop()
	return l
}

// Allow checks if an event from the given IP should be allowed.
// Returns true if the IP is within the rate limit, false if blocked.
//
// A limit of 0 (or negative) disables rate limiting entirely; every call
// returns true. This is useful for deployments behind a single corporate
// NAT where many devices legitimately share one public IP.
func (l *IPLimiter) Allow(ip string) bool {
	if l == nil || l.limit <= 0 {
		return true
	}
	l.mu.Lock()
	defer l.mu.Unlock()

	now := time.Now()
	cutoff := now.Add(-l.window)

	entry, ok := l.entries[ip]
	if !ok {
		entry = &ipEntry{}
		l.entries[ip] = entry
	}

	// Remove expired timestamps
	fresh := entry.timestamps[:0]
	for _, ts := range entry.timestamps {
		if ts.After(cutoff) {
			fresh = append(fresh, ts)
		}
	}
	entry.timestamps = fresh

	// Check if over limit
	if len(entry.timestamps) >= l.limit {
		if !entry.blocked {
			entry.blocked = true
			entry.blockedAt = now
		}
		return false
	}

	// Allow and record
	entry.blocked = false
	entry.timestamps = append(entry.timestamps, now)
	return true
}

// IsBlocked returns true if the IP is currently rate-limited.
func (l *IPLimiter) IsBlocked(ip string) bool {
	l.mu.Lock()
	defer l.mu.Unlock()

	entry, ok := l.entries[ip]
	if !ok {
		return false
	}

	// Prune expired timestamps
	now := time.Now()
	cutoff := now.Add(-l.window)
	fresh := entry.timestamps[:0]
	for _, ts := range entry.timestamps {
		if ts.After(cutoff) {
			fresh = append(fresh, ts)
		}
	}
	entry.timestamps = fresh

	return len(entry.timestamps) >= l.limit
}

// Reset removes all records for a specific IP.
func (l *IPLimiter) Reset(ip string) {
	l.mu.Lock()
	defer l.mu.Unlock()
	delete(l.entries, ip)
}

// Count returns the number of events in the current window for an IP.
func (l *IPLimiter) Count(ip string) int {
	l.mu.Lock()
	defer l.mu.Unlock()

	entry, ok := l.entries[ip]
	if !ok {
		return 0
	}

	cutoff := time.Now().Add(-l.window)
	count := 0
	for _, ts := range entry.timestamps {
		if ts.After(cutoff) {
			count++
		}
	}
	return count
}

// Stats returns the total number of tracked IPs and blocked IPs.
func (l *IPLimiter) Stats() (tracked, blocked int) {
	l.mu.Lock()
	defer l.mu.Unlock()

	tracked = len(l.entries)
	now := time.Now()
	cutoff := now.Add(-l.window)

	for _, entry := range l.entries {
		fresh := 0
		for _, ts := range entry.timestamps {
			if ts.After(cutoff) {
				fresh++
			}
		}
		if fresh >= l.limit {
			blocked++
		}
	}
	return
}

// Stop halts the background cleanup goroutine.
func (l *IPLimiter) Stop() {
	l.mu.Lock()
	defer l.mu.Unlock()
	if !l.stopped {
		l.stopped = true
		close(l.stopCh)
	}
}

// cleanupLoop periodically removes entries that have no recent activity.
func (l *IPLimiter) cleanupLoop() {
	ticker := time.NewTicker(l.cleanup)
	defer ticker.Stop()

	for {
		select {
		case <-l.stopCh:
			return
		case <-ticker.C:
			l.prune()
		}
	}
}

// prune removes IP entries with no timestamps in the current window.
func (l *IPLimiter) prune() {
	l.mu.Lock()
	defer l.mu.Unlock()

	cutoff := time.Now().Add(-l.window)
	for ip, entry := range l.entries {
		fresh := 0
		for _, ts := range entry.timestamps {
			if ts.After(cutoff) {
				fresh++
			}
		}
		if fresh == 0 {
			delete(l.entries, ip)
		}
	}
}

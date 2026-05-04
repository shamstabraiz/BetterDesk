// Package ratelimit provides rate limiting utilities for the Yomie server.
// This file implements a token bucket bandwidth limiter for relay sessions.
package ratelimit

import (
	"io"
	"sync"
	"sync/atomic"
	"time"
)

// BandwidthLimiter controls total and per-session bandwidth for relay connections.
// Uses a token bucket algorithm: tokens are added at a fixed rate and consumed by reads/writes.
type BandwidthLimiter struct {
	totalLimit   int64 // bytes per second (total across all sessions)
	sessionLimit int64 // bytes per second (per session)

	// Global bandwidth tracking
	mu         sync.Mutex
	tokens     int64     // current available tokens (global)
	lastRefill time.Time // last time tokens were refilled

	// Stats
	totalBytes   atomic.Int64 // total bytes transferred
	peakRate     atomic.Int64 // peak bytes/second observed
	activeSess   atomic.Int32 // number of active sessions
	throttleHits atomic.Int64 // number of times throttling was applied
}

// NewBandwidthLimiter creates a new bandwidth limiter.
//
// Parameters:
//   - totalLimit: maximum total bytes per second across all sessions
//   - sessionLimit: maximum bytes per second per session
func NewBandwidthLimiter(totalLimit, sessionLimit int64) *BandwidthLimiter {
	return &BandwidthLimiter{
		totalLimit:   totalLimit,
		sessionLimit: sessionLimit,
		tokens:       totalLimit, // Start with a full bucket
		lastRefill:   time.Now(),
	}
}

// WrapReader wraps an io.Reader with per-session bandwidth limiting.
// The returned reader will throttle reads to not exceed the session limit.
func (bl *BandwidthLimiter) WrapReader(r io.Reader) io.Reader {
	bl.activeSess.Add(1)
	return &limitedReader{
		inner:    r,
		limiter:  bl,
		tokens:   bl.sessionLimit,
		lastFill: time.Now(),
		limit:    bl.sessionLimit,
	}
}

// WrapWriter wraps an io.Writer with per-session bandwidth limiting.
func (bl *BandwidthLimiter) WrapWriter(w io.Writer) io.Writer {
	return &limitedWriter{
		inner:    w,
		limiter:  bl,
		tokens:   bl.sessionLimit,
		lastFill: time.Now(),
		limit:    bl.sessionLimit,
	}
}

// SessionDone should be called when a relay session ends.
func (bl *BandwidthLimiter) SessionDone() {
	bl.activeSess.Add(-1)
}

// consumeGlobal attempts to consume n bytes from the global token bucket.
// Returns the number of bytes allowed (may be less than n).
func (bl *BandwidthLimiter) consumeGlobal(n int64) int64 {
	bl.mu.Lock()
	defer bl.mu.Unlock()

	// Refill tokens based on elapsed time
	now := time.Now()
	elapsed := now.Sub(bl.lastRefill).Seconds()
	if elapsed > 0 {
		refill := int64(elapsed * float64(bl.totalLimit))
		bl.tokens += refill
		if bl.tokens > bl.totalLimit {
			bl.tokens = bl.totalLimit
		}
		bl.lastRefill = now
	}

	// Consume available tokens
	if n > bl.tokens {
		n = bl.tokens
		if n <= 0 {
			bl.throttleHits.Add(1)
			return 0
		}
	}
	bl.tokens -= n
	bl.totalBytes.Add(n)
	return n
}

// Stats returns bandwidth limiter statistics.
func (bl *BandwidthLimiter) Stats() BandwidthStats {
	return BandwidthStats{
		TotalBytesTransferred: bl.totalBytes.Load(),
		ActiveSessions:        int(bl.activeSess.Load()),
		ThrottleHits:          bl.throttleHits.Load(),
		TotalLimitBPS:         bl.totalLimit,
		SessionLimitBPS:       bl.sessionLimit,
	}
}

// BandwidthStats holds bandwidth usage statistics.
type BandwidthStats struct {
	TotalBytesTransferred int64 `json:"total_bytes_transferred"`
	ActiveSessions        int   `json:"active_sessions"`
	ThrottleHits          int64 `json:"throttle_hits"`
	TotalLimitBPS         int64 `json:"total_limit_bps"`
	SessionLimitBPS       int64 `json:"session_limit_bps"`
}

// limitedReader wraps an io.Reader with per-session token bucket limiting.
type limitedReader struct {
	inner    io.Reader
	limiter  *BandwidthLimiter
	mu       sync.Mutex
	tokens   int64
	lastFill time.Time
	limit    int64
}

func (lr *limitedReader) Read(p []byte) (int, error) {
	lr.mu.Lock()

	// Refill session tokens
	now := time.Now()
	elapsed := now.Sub(lr.lastFill).Seconds()
	if elapsed > 0 {
		refill := int64(elapsed * float64(lr.limit))
		lr.tokens += refill
		if lr.tokens > lr.limit {
			lr.tokens = lr.limit
		}
		lr.lastFill = now
	}

	// Determine how much we can read
	allowed := int64(len(p))
	if allowed > lr.tokens {
		allowed = lr.tokens
	}
	if allowed <= 0 {
		// Calculate sleep duration until at least 1 byte is available
		sleepDur := time.Duration(float64(time.Second) / float64(lr.limit))
		if sleepDur < time.Millisecond {
			sleepDur = time.Millisecond
		}
		if sleepDur > 100*time.Millisecond {
			sleepDur = 100 * time.Millisecond
		}
		lr.mu.Unlock()
		time.Sleep(sleepDur)
		return 0, nil
	}

	// Check global budget
	globalAllowed := lr.limiter.consumeGlobal(allowed)
	if globalAllowed <= 0 {
		// Calculate sleep based on global rate
		sleepDur := time.Duration(float64(time.Second) / float64(lr.limiter.totalLimit))
		if sleepDur < time.Millisecond {
			sleepDur = time.Millisecond
		}
		if sleepDur > 100*time.Millisecond {
			sleepDur = 100 * time.Millisecond
		}
		lr.mu.Unlock()
		time.Sleep(sleepDur)
		return 0, nil
	}

	lr.tokens -= globalAllowed
	lr.mu.Unlock()

	return lr.inner.Read(p[:globalAllowed])
}

// limitedWriter wraps an io.Writer with per-session token bucket limiting.
type limitedWriter struct {
	inner    io.Writer
	limiter  *BandwidthLimiter
	mu       sync.Mutex
	tokens   int64
	lastFill time.Time
	limit    int64
}

func (lw *limitedWriter) Write(p []byte) (int, error) {
	written := 0
	remaining := p

	for len(remaining) > 0 {
		lw.mu.Lock()

		// Refill session tokens
		now := time.Now()
		elapsed := now.Sub(lw.lastFill).Seconds()
		if elapsed > 0 {
			refill := int64(elapsed * float64(lw.limit))
			lw.tokens += refill
			if lw.tokens > lw.limit {
				lw.tokens = lw.limit
			}
			lw.lastFill = now
		}

		allowed := int64(len(remaining))
		if allowed > lw.tokens {
			allowed = lw.tokens
		}
		if allowed <= 0 {
			// Calculate sleep duration until at least 1 byte is available
			sleepDur := time.Duration(float64(time.Second) / float64(lw.limit))
			if sleepDur < time.Millisecond {
				sleepDur = time.Millisecond
			}
			if sleepDur > 100*time.Millisecond {
				sleepDur = 100 * time.Millisecond
			}
			lw.mu.Unlock()
			time.Sleep(sleepDur)
			continue
		}

		globalAllowed := lw.limiter.consumeGlobal(allowed)
		if globalAllowed <= 0 {
			// Calculate sleep based on global rate
			sleepDur := time.Duration(float64(time.Second) / float64(lw.limiter.totalLimit))
			if sleepDur < time.Millisecond {
				sleepDur = time.Millisecond
			}
			if sleepDur > 100*time.Millisecond {
				sleepDur = 100 * time.Millisecond
			}
			lw.mu.Unlock()
			time.Sleep(sleepDur)
			continue
		}

		lw.tokens -= globalAllowed
		lw.mu.Unlock()

		n, err := lw.inner.Write(remaining[:globalAllowed])
		written += n
		if err != nil {
			return written, err
		}
		remaining = remaining[n:]
	}

	return written, nil
}

package ratelimit

import (
	"testing"
	"time"
)

func TestIPLimiterAllow(t *testing.T) {
	l := NewIPLimiter(5, 1*time.Minute, 5*time.Minute)
	defer l.Stop()

	ip := "192.168.1.1"

	// First 5 should be allowed
	for i := 0; i < 5; i++ {
		if !l.Allow(ip) {
			t.Fatalf("request %d should be allowed", i+1)
		}
	}

	// 6th should be blocked
	if l.Allow(ip) {
		t.Fatal("6th request should be blocked")
	}

	// Different IP should still be allowed
	if !l.Allow("10.0.0.1") {
		t.Fatal("different IP should be allowed")
	}
}

func TestIPLimiterSlidingWindow(t *testing.T) {
	l := NewIPLimiter(3, 100*time.Millisecond, 5*time.Second)
	defer l.Stop()

	ip := "10.0.0.5"

	// Use up the limit
	for i := 0; i < 3; i++ {
		l.Allow(ip)
	}
	if l.Allow(ip) {
		t.Fatal("should be blocked at limit")
	}

	// Wait for window to expire
	time.Sleep(150 * time.Millisecond)

	// Should be allowed again
	if !l.Allow(ip) {
		t.Fatal("should be allowed after window expires")
	}
}

func TestIPLimiterIsBlocked(t *testing.T) {
	l := NewIPLimiter(2, 1*time.Minute, 5*time.Minute)
	defer l.Stop()

	ip := "172.16.0.1"

	if l.IsBlocked(ip) {
		t.Fatal("should not be blocked initially")
	}

	l.Allow(ip)
	l.Allow(ip)

	if !l.IsBlocked(ip) {
		t.Fatal("should be blocked after reaching limit")
	}
}

func TestIPLimiterReset(t *testing.T) {
	l := NewIPLimiter(2, 1*time.Minute, 5*time.Minute)
	defer l.Stop()

	ip := "10.10.10.10"
	l.Allow(ip)
	l.Allow(ip)

	if !l.IsBlocked(ip) {
		t.Fatal("should be blocked")
	}

	l.Reset(ip)

	if l.IsBlocked(ip) {
		t.Fatal("should not be blocked after reset")
	}
	if l.Count(ip) != 0 {
		t.Fatalf("count should be 0 after reset, got %d", l.Count(ip))
	}
}

func TestIPLimiterCount(t *testing.T) {
	l := NewIPLimiter(10, 1*time.Minute, 5*time.Minute)
	defer l.Stop()

	ip := "8.8.8.8"
	for i := 0; i < 7; i++ {
		l.Allow(ip)
	}

	if count := l.Count(ip); count != 7 {
		t.Fatalf("count: got %d, want 7", count)
	}
}

func TestIPLimiterStats(t *testing.T) {
	l := NewIPLimiter(3, 1*time.Minute, 5*time.Minute)
	defer l.Stop()

	// Add 2 IPs: one blocked, one not
	for i := 0; i < 3; i++ {
		l.Allow("1.1.1.1")
	}
	l.Allow("2.2.2.2")

	tracked, blocked := l.Stats()
	if tracked != 2 {
		t.Errorf("tracked: got %d, want 2", tracked)
	}
	if blocked != 1 {
		t.Errorf("blocked: got %d, want 1", blocked)
	}
}

func TestIPLimiterPrune(t *testing.T) {
	l := NewIPLimiter(5, 50*time.Millisecond, 5*time.Second)
	defer l.Stop()

	l.Allow("prune-test")

	time.Sleep(100 * time.Millisecond)

	l.prune()

	l.mu.Lock()
	_, exists := l.entries["prune-test"]
	l.mu.Unlock()

	if exists {
		t.Fatal("entry should have been pruned")
	}
}

func BenchmarkIPLimiterAllow(b *testing.B) {
	l := NewIPLimiter(100, 1*time.Minute, 5*time.Minute)
	defer l.Stop()

	b.RunParallel(func(pb *testing.PB) {
		for pb.Next() {
			l.Allow("bench-ip")
		}
	})
}

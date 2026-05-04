package metrics

import (
	"bytes"
	"strings"
	"testing"
)

func TestWritePrometheus(t *testing.T) {
	c := NewCollector()

	c.PeersTotal.Store(100)
	c.PeersOnline.Store(42)
	c.PeersDegraded.Store(3)
	c.TotalRegistrations.Store(500)
	c.RelayActiveSessions.Store(5)
	c.TotalRelaySessions.Store(200)

	var buf bytes.Buffer
	c.WritePrometheus(&buf)

	output := buf.String()

	// Should contain HELP and TYPE lines
	if !strings.Contains(output, "# HELP betterdesk_peers_total") {
		t.Error("missing HELP for peers_total")
	}
	if !strings.Contains(output, "# TYPE betterdesk_peers_total gauge") {
		t.Error("missing TYPE for peers_total")
	}
	if !strings.Contains(output, "betterdesk_peers_total 100") {
		t.Error("missing peers_total value")
	}
	if !strings.Contains(output, "betterdesk_peers_online 42") {
		t.Error("missing peers_online value")
	}
	if !strings.Contains(output, "betterdesk_registrations_total 500") {
		t.Error("missing registrations_total value")
	}
	if !strings.Contains(output, "betterdesk_relay_active_sessions 5") {
		t.Error("missing relay_active_sessions value")
	}

	// Should contain uptime
	if !strings.Contains(output, "betterdesk_uptime_seconds") {
		t.Error("missing uptime_seconds")
	}

	// Should contain goroutines (runtime metric)
	if !strings.Contains(output, "betterdesk_goroutines") {
		t.Error("missing goroutines metric")
	}
}

func TestCounterIncrement(t *testing.T) {
	c := NewCollector()
	c.TotalRegistrations.Add(1)
	c.TotalRegistrations.Add(1)
	c.TotalRegistrations.Add(1)

	if c.TotalRegistrations.Load() != 3 {
		t.Errorf("expected 3, got %d", c.TotalRegistrations.Load())
	}
}

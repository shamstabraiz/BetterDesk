// Package security provides IP/ID blocklist management for the Yomie server.
// Supports loading from files, runtime additions, and both IP and ID blocking.
package security

import (
	"bufio"
	"log"
	"net"
	"os"
	"strings"
	"sync"
	"time"
)

// BlockType identifies what kind of entry is blocked.
type BlockType string

const (
	BlockIP BlockType = "ip"
	BlockID BlockType = "id"
	BlockNet BlockType = "cidr"
)

// BlockEntry represents a single blocklist entry.
type BlockEntry struct {
	Value     string    `json:"value"`      // IP, CIDR, or device ID
	Type      BlockType `json:"type"`       // ip, id, or cidr
	Reason    string    `json:"reason"`     // Why it was blocked
	CreatedAt time.Time `json:"created_at"` // When it was added
}

// Blocklist manages blocked IPs, CIDRs, and device IDs.
// Thread-safe — uses sync.RWMutex for concurrent access.
type Blocklist struct {
	mu       sync.RWMutex
	ips      map[string]*BlockEntry  // Exact IP matches
	ids      map[string]*BlockEntry  // Device ID matches
	networks []*networkEntry         // CIDR network matches
	filePath string                  // Path to blocklist file (optional)
}

type networkEntry struct {
	net   *net.IPNet
	entry *BlockEntry
}

// NewBlocklist creates a new empty blocklist.
func NewBlocklist() *Blocklist {
	return &Blocklist{
		ips: make(map[string]*BlockEntry),
		ids: make(map[string]*BlockEntry),
	}
}

// LoadFromFile loads entries from a blocklist file.
// File format: one entry per line, lines starting with # are comments.
// Lines can be: bare IPs, CIDR notation, or device IDs prefixed with "id:".
// Optional reason after a comma: "192.168.1.1,brute force attempt"
func (b *Blocklist) LoadFromFile(path string) error {
	f, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil // File doesn't exist — not an error
		}
		return err
	}
	defer f.Close()

	b.mu.Lock()
	defer b.mu.Unlock()

	b.filePath = path
	scanner := bufio.NewScanner(f)
	loaded := 0
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}

		// Split value and optional reason
		value, reason := line, ""
		if idx := strings.Index(line, ","); idx >= 0 {
			value = strings.TrimSpace(line[:idx])
			reason = strings.TrimSpace(line[idx+1:])
		}

		entry := &BlockEntry{
			Value:     value,
			Reason:    reason,
			CreatedAt: time.Now(),
		}

		if strings.HasPrefix(value, "id:") {
			entry.Value = strings.TrimPrefix(value, "id:")
			entry.Type = BlockID
			b.ids[entry.Value] = entry
		} else if strings.Contains(value, "/") {
			_, ipNet, err := net.ParseCIDR(value)
			if err != nil {
				log.Printf("[blocklist] Invalid CIDR %q: %v", value, err)
				continue
			}
			entry.Type = BlockNet
			b.networks = append(b.networks, &networkEntry{net: ipNet, entry: entry})
		} else {
			entry.Type = BlockIP
			b.ips[value] = entry
		}
		loaded++
	}

	if loaded > 0 {
		log.Printf("[blocklist] Loaded %d entries from %s", loaded, path)
	}
	return scanner.Err()
}

// IsIPBlocked checks if an IP address is blocked (exact or CIDR match).
func (b *Blocklist) IsIPBlocked(ipStr string) bool {
	b.mu.RLock()
	defer b.mu.RUnlock()

	// Strip port if present
	host, _, err := net.SplitHostPort(ipStr)
	if err != nil {
		host = ipStr
	}

	// Exact match
	if _, ok := b.ips[host]; ok {
		return true
	}

	// CIDR match
	ip := net.ParseIP(host)
	if ip == nil {
		return false
	}
	for _, ne := range b.networks {
		if ne.net.Contains(ip) {
			return true
		}
	}

	return false
}

// IsIDBlocked checks if a device ID is blocked.
func (b *Blocklist) IsIDBlocked(id string) bool {
	b.mu.RLock()
	defer b.mu.RUnlock()
	_, ok := b.ids[id]
	return ok
}

// BlockIP adds an IP to the blocklist at runtime.
func (b *Blocklist) BlockIP(ip, reason string) {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.ips[ip] = &BlockEntry{
		Value:     ip,
		Type:      BlockIP,
		Reason:    reason,
		CreatedAt: time.Now(),
	}
}

// BlockID adds a device ID to the blocklist at runtime.
func (b *Blocklist) BlockID(id, reason string) {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.ids[id] = &BlockEntry{
		Value:     id,
		Type:      BlockID,
		Reason:    reason,
		CreatedAt: time.Now(),
	}
}

// BlockCIDR adds a CIDR network to the blocklist at runtime.
func (b *Blocklist) BlockCIDR(cidr, reason string) error {
	_, ipNet, err := net.ParseCIDR(cidr)
	if err != nil {
		return err
	}

	b.mu.Lock()
	defer b.mu.Unlock()
	b.networks = append(b.networks, &networkEntry{
		net: ipNet,
		entry: &BlockEntry{
			Value:     cidr,
			Type:      BlockNet,
			Reason:    reason,
			CreatedAt: time.Now(),
		},
	})
	return nil
}

// UnblockIP removes an IP from the blocklist.
func (b *Blocklist) UnblockIP(ip string) bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	if _, ok := b.ips[ip]; ok {
		delete(b.ips, ip)
		return true
	}
	return false
}

// UnblockID removes a device ID from the blocklist.
func (b *Blocklist) UnblockID(id string) bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	if _, ok := b.ids[id]; ok {
		delete(b.ids, id)
		return true
	}
	return false
}

// UnblockCIDR removes a CIDR from the blocklist.
func (b *Blocklist) UnblockCIDR(cidr string) bool {
	b.mu.Lock()
	defer b.mu.Unlock()
	for i, ne := range b.networks {
		if ne.entry.Value == cidr {
			b.networks = append(b.networks[:i], b.networks[i+1:]...)
			return true
		}
	}
	return false
}

// List returns all current blocklist entries.
func (b *Blocklist) List() []BlockEntry {
	b.mu.RLock()
	defer b.mu.RUnlock()

	var entries []BlockEntry
	for _, e := range b.ips {
		entries = append(entries, *e)
	}
	for _, e := range b.ids {
		entries = append(entries, *e)
	}
	for _, ne := range b.networks {
		entries = append(entries, *ne.entry)
	}
	return entries
}

// Count returns the total number of blocklist entries.
func (b *Blocklist) Count() int {
	b.mu.RLock()
	defer b.mu.RUnlock()
	return len(b.ips) + len(b.ids) + len(b.networks)
}

// SaveToFile writes the current blocklist to the file.
func (b *Blocklist) SaveToFile(path string) error {
	b.mu.RLock()
	defer b.mu.RUnlock()

	if path == "" {
		path = b.filePath
	}
	if path == "" {
		return nil
	}

	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()

	w := bufio.NewWriter(f)
	w.WriteString("# Yomie Blocklist (auto-generated)\n")
	w.WriteString("# Format: IP, CIDR, or id:DEVICE_ID [,reason]\n\n")

	for _, e := range b.ips {
		line := e.Value
		if e.Reason != "" {
			line += "," + e.Reason
		}
		w.WriteString(line + "\n")
	}
	for _, ne := range b.networks {
		line := ne.entry.Value
		if ne.entry.Reason != "" {
			line += "," + ne.entry.Reason
		}
		w.WriteString(line + "\n")
	}
	for _, e := range b.ids {
		line := "id:" + e.Value
		if e.Reason != "" {
			line += "," + e.Reason
		}
		w.WriteString(line + "\n")
	}

	return w.Flush()
}

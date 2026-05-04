package agent

import (
	"context"
	"log"
	"runtime"
	"time"

	"github.com/shirou/gopsutil/v3/cpu"
	"github.com/shirou/gopsutil/v3/disk"
	"github.com/shirou/gopsutil/v3/host"
	"github.com/shirou/gopsutil/v3/mem"
)

// MetricsData mirrors the server-side MetricsData struct.
type MetricsData struct {
	CPU    float64 `json:"cpu"`
	Memory float64 `json:"memory"`
	Disk   float64 `json:"disk"`
}

// SystemInfo holds static system information used for manifest building.
type SystemInfo struct {
	Hostname        string
	OS              string
	Platform        string
	PlatformVersion string
	Arch            string
	Uptime          uint64 // seconds
	TotalMemory     uint64 // bytes
	TotalDisk       uint64 // bytes
}

// SystemCollector gathers host metrics using gopsutil.
type SystemCollector struct {
	cachedInfo *SystemInfo
}

// NewSystemCollector creates a new collector that lazily caches static info.
func NewSystemCollector() *SystemCollector {
	return &SystemCollector{}
}

// Collect returns current CPU, memory, and disk usage percentages.
func (sc *SystemCollector) Collect() *MetricsData {
	m := &MetricsData{}

	// CPU (1-second sample)
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	if pcts, err := cpu.PercentWithContext(ctx, time.Second, false); err == nil && len(pcts) > 0 {
		m.CPU = pcts[0]
	}

	// Memory
	if vm, err := mem.VirtualMemory(); err == nil {
		m.Memory = vm.UsedPercent
	}

	// Disk (root partition)
	rootPath := "/"
	if runtime.GOOS == "windows" {
		rootPath = "C:\\"
	}
	if du, err := disk.Usage(rootPath); err == nil {
		m.Disk = du.UsedPercent
	}

	return m
}

// GetInfo returns static system information, cached after first call.
func (sc *SystemCollector) GetInfo() *SystemInfo {
	if sc.cachedInfo != nil {
		return sc.cachedInfo
	}

	info := &SystemInfo{Arch: runtime.GOARCH}

	if hi, err := host.Info(); err == nil {
		info.Hostname = hi.Hostname
		info.OS = hi.OS
		info.Platform = hi.Platform
		info.PlatformVersion = hi.PlatformVersion
		info.Uptime = hi.Uptime
	}

	if vm, err := mem.VirtualMemory(); err == nil {
		info.TotalMemory = vm.Total
	}

	rootPath := "/"
	if runtime.GOOS == "windows" {
		rootPath = "C:\\"
	}
	if du, err := disk.Usage(rootPath); err == nil {
		info.TotalDisk = du.Total
	}

	sc.cachedInfo = info
	return info
}

// Uptime returns the current system uptime in seconds.
func (sc *SystemCollector) Uptime() uint64 {
	if hi, err := host.Info(); err == nil {
		return hi.Uptime
	}
	return 0
}

// CaptureScreenshot takes a screenshot using OS-specific commands.
// Returns JPEG bytes or an error. This is a best-effort function.
func CaptureScreenshot() ([]byte, error) {
	return captureScreenshotPlatform()
}

func init() {
	// Suppress gopsutil warnings on systems without certain features
	log.SetFlags(log.LstdFlags | log.Lshortfile)
}

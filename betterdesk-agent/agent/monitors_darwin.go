//go:build darwin

package agent

import (
	"os/exec"
	"regexp"
	"strconv"
	"strings"
)

// enumerateMonitors uses `system_profiler SPDisplaysDataType` to list the
// displays attached to a Mac. The fields we extract are robust against
// Apple's continual format churn — only Resolution and the human display
// name are required.
func enumerateMonitors() []MonitorInfo {
	out, err := exec.Command("system_profiler", "SPDisplaysDataType").Output()
	if err != nil {
		return []MonitorInfo{{Index: 0, Name: "Display", Primary: true}}
	}

	var mons []MonitorInfo
	resRe := regexp.MustCompile(`Resolution:\s+(\d+)\s*x\s*(\d+)`)
	mainRe := regexp.MustCompile(`Main Display:\s+Yes`)

	// Split on blank lines between displays.
	blocks := strings.Split(string(out), "\n\n")
	idx := 0
	for _, blk := range blocks {
		if !strings.Contains(blk, "Resolution:") {
			continue
		}
		// First non-blank, non-indented colon line is the display name.
		var name string
		for _, line := range strings.Split(blk, "\n") {
			t := strings.TrimRight(line, " :\t")
			if t == "" || strings.HasPrefix(line, " ") {
				continue
			}
			name = strings.TrimSuffix(t, ":")
			break
		}
		w, h := 0, 0
		if m := resRe.FindStringSubmatch(blk); m != nil {
			w, _ = strconv.Atoi(m[1])
			h, _ = strconv.Atoi(m[2])
		}
		mons = append(mons, MonitorInfo{
			Index:   idx,
			Name:    name,
			Width:   w,
			Height:  h,
			Primary: mainRe.MatchString(blk),
		})
		idx++
	}
	if len(mons) == 0 {
		return []MonitorInfo{{Index: 0, Name: "Display", Primary: true}}
	}
	return mons
}

// desktopCaptureHint returns guidance for fixing screen capture on macOS.
func desktopCaptureHint() string {
	return "Grant 'Screen Recording' permission to BetterDesk in System Settings → Privacy & Security → Screen Recording, then restart the agent."
}

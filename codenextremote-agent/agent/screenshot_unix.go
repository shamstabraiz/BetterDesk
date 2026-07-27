//go:build !windows

package agent

import (
	"fmt"
	"os"
	"os/exec"
)

// captureScreenshotPlatform captures a screenshot on Linux/macOS using
// available command-line tools. Returns JPEG bytes.
//
// Linux tool priority:
//  1. Wayland: grim → wayshot → gnome-screenshot (portal-based, modern)
//  2. X11/XWayland: scrot → import (ImageMagick) → gnome-screenshot
//
// macOS:
//  1. screencapture (built-in, requires Screen Recording permission on 10.15+)
func captureScreenshotPlatform() ([]byte, error) {
	// macOS: screencapture is always the right tool; try it first when on darwin.
	if path, err := exec.LookPath("screencapture"); err == nil {
		// -x = no shutter sound, -t jpg = JPEG format, - = write to stdout
		cmd := exec.Command(path, "-x", "-t", "jpg", "-")
		out, err := cmd.Output()
		if err == nil && len(out) > 0 {
			return out, nil
		}
		// Non-nil error usually means Screen Recording permission was denied.
		if err != nil {
			return nil, fmt.Errorf(
				"screencapture failed (Screen Recording permission may be denied — open System Settings > Privacy & Security > Screen Recording): %v",
				err,
			)
		}
	}

	// Linux: Wayland-native tools first when a Wayland session is active.
	if os.Getenv("WAYLAND_DISPLAY") != "" && os.Getenv("DISPLAY") == "" {
		// grim: wlroots / sway / labwc
		if path, err := exec.LookPath("grim"); err == nil {
			cmd := exec.Command(path, "-")
			out, err := cmd.Output()
			if err == nil && len(out) > 0 {
				return out, nil
			}
		}

		// wayshot: another lightweight wlroots screenshotter
		if path, err := exec.LookPath("wayshot"); err == nil {
			cmd := exec.Command(path, "--stdout")
			out, err := cmd.Output()
			if err == nil && len(out) > 0 {
				return out, nil
			}
		}

		// GNOME / KDE Wayland (XDG Desktop Portal)
		if path, err := exec.LookPath("gnome-screenshot"); err == nil {
			cmd := exec.Command(path, "-f", "/dev/stdout")
			out, err := cmd.Output()
			if err == nil && len(out) > 0 {
				return out, nil
			}
		}
	}

	// Linux X11 / XWayland path:
	// scrot writes to stdout when given "-" as filename.
	if path, err := exec.LookPath("scrot"); err == nil {
		cmd := exec.Command(path, "-o", "-", "--quality", "80")
		out, err := cmd.Output()
		if err == nil && len(out) > 0 {
			return out, nil
		}
	}

	// ImageMagick import
	if path, err := exec.LookPath("import"); err == nil {
		cmd := exec.Command(path, "-window", "root", "jpeg:-")
		out, err := cmd.Output()
		if err == nil && len(out) > 0 {
			return out, nil
		}
	}

	// Last resort: gnome-screenshot on X11
	if path, err := exec.LookPath("gnome-screenshot"); err == nil {
		cmd := exec.Command(path, "-f", "/dev/stdout")
		out, err := cmd.Output()
		if err == nil && len(out) > 0 {
			return out, nil
		}
	}

	return nil, fmt.Errorf("no screenshot tool available — install one of: scrot, grim (Wayland), ImageMagick")
}

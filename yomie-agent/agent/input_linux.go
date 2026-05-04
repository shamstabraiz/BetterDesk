//go:build linux

package agent

import (
	"fmt"
	"os/exec"
	"strings"
)

// injectInput injects a keyboard or mouse event on Linux.
//
// Strategy:
//  1. X11 or XWayland session (DISPLAY set) → use xdotool
//  2. Pure Wayland without XWayland (DISPLAY not set) → use ydotool
//     (requires ydotoold daemon to be running)
//
// Most Wayland compositors (GNOME, KDE, sway) run XWayland, so path 1
// is taken even on Wayland desktops in most real-world setups.
func injectInput(evt *InputEvent) error {
	if hasX11Display() {
		return injectInputX11(evt)
	}
	return injectInputWayland(evt)
}

// ── X11 / XWayland path (xdotool) ────────────────────────────────────────

func injectInputX11(evt *InputEvent) error {
	switch evt.Type {
	case "mouse_move":
		return xdotool("mousemove", "--sync",
			fmt.Sprintf("%d", evt.X), fmt.Sprintf("%d", evt.Y))

	case "mouse_click":
		if err := xdotool("mousemove", "--sync",
			fmt.Sprintf("%d", evt.X), fmt.Sprintf("%d", evt.Y)); err != nil {
			return err
		}
		return xdotool("click", fmt.Sprintf("%d", linuxMouseButton(evt.Button)))

	case "mouse_down":
		return xdotool("mousedown", fmt.Sprintf("%d", linuxMouseButton(evt.Button)))

	case "mouse_up":
		return xdotool("mouseup", fmt.Sprintf("%d", linuxMouseButton(evt.Button)))

	case "mouse_scroll":
		if evt.DeltaY < 0 {
			return xdotool("click", "--repeat", fmt.Sprintf("%d", abs(evt.DeltaY)), "4")
		} else if evt.DeltaY > 0 {
			return xdotool("click", "--repeat", fmt.Sprintf("%d", abs(evt.DeltaY)), "5")
		}
		if evt.DeltaX < 0 {
			return xdotool("click", "--repeat", fmt.Sprintf("%d", abs(evt.DeltaX)), "6")
		} else if evt.DeltaX > 0 {
			return xdotool("click", "--repeat", fmt.Sprintf("%d", abs(evt.DeltaX)), "7")
		}
		return nil

	case "key_press":
		return xdotool("keydown", buildXdotoolKeyCombo(evt.Key, evt.Modifiers))

	case "key_release":
		return xdotool("keyup", buildXdotoolKeyCombo(evt.Key, evt.Modifiers))

	case "key_tap":
		return xdotool("key", buildXdotoolKeyCombo(evt.Key, evt.Modifiers))

	case "text":
		if evt.Text == "" {
			return nil
		}
		return xdotool("type", "--clearmodifiers", "--", evt.Text)

	default:
		return fmt.Errorf("unknown input type: %s", evt.Type)
	}
}

// xdotool runs xdotool with the given arguments.
func xdotool(args ...string) error {
	path, err := exec.LookPath("xdotool")
	if err != nil {
		return fmt.Errorf("xdotool not found — install it with: sudo apt install xdotool")
	}
	return exec.Command(path, args...).Run()
}

// ── Pure-Wayland path (ydotool) ──────────────────────────────────────────

// injectInputWayland uses ydotool for input injection on pure-Wayland sessions.
//
// Requirements:
//   - ydotool installed:       sudo apt install ydotool   (or build from source)
//   - ydotoold daemon running: sudo ydotoold &
//
// ydotool 1.x command syntax is used here. On Debian/Ubuntu the package may be
// older (0.x); if commands fail, upgrade or use XWayland instead.
func injectInputWayland(evt *InputEvent) error {
	switch evt.Type {
	case "mouse_move":
		return ydotool("mousemove",
			"-x", fmt.Sprintf("%d", evt.X),
			"-y", fmt.Sprintf("%d", evt.Y))

	case "mouse_click":
		if err := ydotool("mousemove",
			"-x", fmt.Sprintf("%d", evt.X),
			"-y", fmt.Sprintf("%d", evt.Y)); err != nil {
			return err
		}
		return ydotoolClick(evt.Button)

	case "mouse_down":
		return ydotoolMouseDown(evt.Button)

	case "mouse_up":
		return ydotoolMouseUp(evt.Button)

	case "mouse_scroll":
		// ydotool scroll: positive DeltaY = scroll down
		if evt.DeltaY != 0 {
			// ydotool scroll button clicks: 4=wheel-up, 5=wheel-down
			btn := "5"
			repeat := evt.DeltaY
			if evt.DeltaY < 0 {
				btn = "4"
				repeat = -repeat
			}
			for i := 0; i < repeat; i++ {
				if err := ydotoolClick(parseScrollButton(btn)); err != nil {
					return err
				}
			}
		}
		return nil

	case "key_press":
		return ydotool("key", "--key-delay", "0",
			buildYdotoolKey(evt.Key, evt.Modifiers))

	case "key_release":
		// ydotool does not support individual key-up; send a no-op
		return nil

	case "key_tap":
		return ydotool("key", "--key-delay", "12",
			buildYdotoolKey(evt.Key, evt.Modifiers))

	case "text":
		if evt.Text == "" {
			return nil
		}
		// wtype is more reliable than ydotool type on many compositors.
		if path, err := exec.LookPath("wtype"); err == nil {
			return exec.Command(path, "-d", "0", evt.Text).Run()
		}
		return ydotool("type", "--key-delay", "12", "--", evt.Text)

	default:
		return fmt.Errorf("unknown input type: %s", evt.Type)
	}
}

// ydotool runs ydotool with the given arguments.
func ydotool(args ...string) error {
	path, err := exec.LookPath("ydotool")
	if err != nil {
		return fmt.Errorf(
			"ydotool not found — install it and start ydotoold: sudo apt install ydotool && sudo ydotoold &",
		)
	}
	return exec.Command(path, args...).Run()
}

// ydotoolClick sends a full button click (down + up) for a CDAP button number.
// CDAP: 1=left, 2=right, 3=middle.  ydotool uses the same numbering as xdotool.
func ydotoolClick(button int) error {
	return ydotool("click", fmt.Sprintf("%d", linuxMouseButton(button)))
}

// ydotoolMouseDown presses a mouse button.
func ydotoolMouseDown(button int) error {
	// ydotool does not have separate mousedown/mouseup in all versions;
	// fall back to a click as a best-effort.
	return ydotoolClick(button)
}

// ydotoolMouseUp releases a mouse button (best-effort no-op for ydotool).
func ydotoolMouseUp(_ int) error {
	return nil
}

// parseScrollButton converts a scroll button string ("4" or "5") to int.
func parseScrollButton(btn string) int {
	if btn == "4" {
		return 4
	}
	return 5
}

// buildYdotoolKey constructs a ydotool key combo string using XKB key names.
// ydotool accepts the same modifier+key syntax as xdotool.
func buildYdotoolKey(key string, modifiers []string) string {
	return buildXdotoolKeyCombo(key, modifiers)
}

// ── Shared helpers ────────────────────────────────────────────────────────

// linuxMouseButton maps CDAP button numbers to Linux tool button numbers.
// CDAP: 1=left, 2=right, 3=middle. xdotool/ydotool: 1=left, 2=middle, 3=right.
func linuxMouseButton(btn int) int {
	switch btn {
	case 1:
		return 1 // left
	case 2:
		return 3 // right
	case 3:
		return 2 // middle
	default:
		return 1
	}
}

// buildXdotoolKeyCombo constructs an xdotool/ydotool key combination string.
// Example: key="Return", modifiers=["ctrl"] → "ctrl+Return".
func buildXdotoolKeyCombo(key string, modifiers []string) string {
	var parts []string
	for _, mod := range modifiers {
		parts = append(parts, normalizeModifier(mod))
	}
	if key != "" {
		parts = append(parts, xdotoolKeyName(key))
	}
	return strings.Join(parts, "+")
}

// normalizeModifier maps CDAP modifier names to xdotool/ydotool names.
func normalizeModifier(mod string) string {
	switch strings.ToLower(mod) {
	case "ctrl", "control":
		return "ctrl"
	case "alt":
		return "alt"
	case "shift":
		return "shift"
	case "super", "meta", "win", "cmd":
		return "super"
	default:
		return mod
	}
}

// xdotoolKeyName maps CDAP key names to xdotool/ydotool key symbol names.
func xdotoolKeyName(key string) string {
	switch key {
	case "Return", "Enter":
		return "Return"
	case "Backspace":
		return "BackSpace"
	case "Delete", "Del":
		return "Delete"
	case "Escape", "Esc":
		return "Escape"
	case "Tab":
		return "Tab"
	case "Space", " ":
		return "space"
	case "ArrowUp", "Up":
		return "Up"
	case "ArrowDown", "Down":
		return "Down"
	case "ArrowLeft", "Left":
		return "Left"
	case "ArrowRight", "Right":
		return "Right"
	case "Home":
		return "Home"
	case "End":
		return "End"
	case "PageUp":
		return "Prior"
	case "PageDown":
		return "Next"
	case "F1":
		return "F1"
	case "F2":
		return "F2"
	case "F3":
		return "F3"
	case "F4":
		return "F4"
	case "F5":
		return "F5"
	case "F6":
		return "F6"
	case "F7":
		return "F7"
	case "F8":
		return "F8"
	case "F9":
		return "F9"
	case "F10":
		return "F10"
	case "F11":
		return "F11"
	case "F12":
		return "F12"
	case "PrintScreen":
		return "Print"
	case "Insert":
		return "Insert"
	case "CapsLock":
		return "Caps_Lock"
	case "NumLock":
		return "Num_Lock"
	default:
		return key
	}
}

func abs(x int) int {
	if x < 0 {
		return -x
	}
	return x
}

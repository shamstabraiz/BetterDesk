//go:build darwin

package agent

import (
	"fmt"
	"os/exec"
	"strings"
)

// injectInput injects keyboard/mouse events on macOS.
// Uses cliclick (https://github.com/BlueM/cliclick) for mouse events and
// osascript for keyboard events. cliclick must be installed:
//
//	brew install cliclick
func injectInput(evt *InputEvent) error {
	switch evt.Type {
	case "mouse_move":
		return cliclick(fmt.Sprintf("m:%d,%d", evt.X, evt.Y))

	case "mouse_click":
		if err := cliclick(fmt.Sprintf("m:%d,%d", evt.X, evt.Y)); err != nil {
			return err
		}
		return cliclick(darwinClickAction(evt.Button, false))

	case "mouse_down":
		return cliclick(darwinClickAction(evt.Button, false))

	case "mouse_up":
		return cliclick(darwinClickAction(evt.Button, true))

	case "mouse_scroll":
		// cliclick does not support scroll; use osascript
		if evt.DeltaY != 0 {
			dir := "down"
			if evt.DeltaY < 0 {
				dir = "up"
			}
			return osascript(fmt.Sprintf(`tell application "System Events" to scroll %s %d`, dir, abs(evt.DeltaY)))
		}
		return nil

	case "key_tap":
		key, mods := darwinKey(evt.Key, evt.Modifiers)
		if key == "" {
			return nil
		}
		script := buildKeyScript(key, mods, true, true)
		return osascript(script)

	case "key_press":
		key, mods := darwinKey(evt.Key, evt.Modifiers)
		if key == "" {
			return nil
		}
		return osascript(buildKeyScript(key, mods, true, false))

	case "key_release":
		// osascript doesn't support key-up — ignore
		return nil

	case "text":
		if evt.Text == "" {
			return nil
		}
		escaped := strings.ReplaceAll(evt.Text, `"`, `\"`)
		return osascript(fmt.Sprintf(`tell application "System Events" to keystroke "%s"`, escaped))

	default:
		return fmt.Errorf("unknown input type: %s", evt.Type)
	}
}

func cliclick(args ...string) error {
	path, err := exec.LookPath("cliclick")
	if err != nil {
		return fmt.Errorf(
			"cliclick not found — mouse injection requires cliclick: brew install cliclick\n" +
				"Also enable Accessibility permission in System Settings > Privacy & Security > Accessibility",
		)
	}
	out, err := exec.Command(path, args...).CombinedOutput()
	if err != nil {
		// Detect Accessibility permission denial (common on macOS 14+).
		if strings.Contains(string(out), "permission") || strings.Contains(string(out), "not allowed") {
			return fmt.Errorf(
				"cliclick access denied — enable Accessibility permission in System Settings > Privacy & Security > Accessibility for BetterDesk Agent",
			)
		}
		return fmt.Errorf("cliclick: %w (output: %s)", err, string(out))
	}
	return nil
}

func osascript(script string) error {
	return exec.Command("osascript", "-e", script).Run()
}

func darwinClickAction(button int, up bool) string {
	switch button {
	case 1:
		if up {
			return "ku:."
		}
		return "kd:."
	case 2:
		if up {
			return "rc:."
		}
		return "rc:."
	default:
		if up {
			return "ku:."
		}
		return "kd:."
	}
}

var darwinKeyMap = map[string]string{
	"Return": "return", "Enter": "return",
	"Backspace": "delete",
	"Delete":    "forward delete", "Del": "forward delete",
	"Escape": "escape", "Esc": "escape",
	"Tab":     "tab",
	"Space":   "space",
	"ArrowUp": "up arrow", "Up": "up arrow",
	"ArrowDown": "down arrow", "Down": "down arrow",
	"ArrowLeft": "left arrow", "Left": "left arrow",
	"ArrowRight": "right arrow", "Right": "right arrow",
	"Home":     "home",
	"End":      "end",
	"PageUp":   "page up",
	"PageDown": "page down",
	"F1":       "F1", "F2": "F2", "F3": "F3", "F4": "F4",
	"F5": "F5", "F6": "F6", "F7": "F7", "F8": "F8",
	"F9": "F9", "F10": "F10", "F11": "F11", "F12": "F12",
}

func darwinKey(key string, modifiers []string) (string, []string) {
	k, ok := darwinKeyMap[key]
	if !ok {
		if len(key) == 1 {
			k = key
		} else {
			return "", nil
		}
	}

	var mods []string
	for _, mod := range modifiers {
		switch strings.ToLower(mod) {
		case "ctrl", "control":
			mods = append(mods, "control down")
		case "alt":
			mods = append(mods, "option down")
		case "shift":
			mods = append(mods, "shift down")
		case "super", "meta", "cmd":
			mods = append(mods, "command down")
		}
	}
	return k, mods
}

func buildKeyScript(key string, mods []string, press, release bool) string {
	using := ""
	if len(mods) > 0 {
		using = " using {" + strings.Join(mods, ", ") + "}"
	}

	if len(key) == 1 {
		return fmt.Sprintf(`tell application "System Events" to keystroke "%s"%s`, key, using)
	}
	return fmt.Sprintf(`tell application "System Events" to key code (get key code of "%s")%s`, key, using)
}

func abs(x int) int {
	if x < 0 {
		return -x
	}
	return x
}

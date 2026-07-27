//go:build windows

package agent

import (
	"fmt"
	"strings"
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	user32               = windows.NewLazySystemDLL("user32.dll")
	procSendInput        = user32.NewProc("SendInput")
	procSetCursorPos     = user32.NewProc("SetCursorPos")
	procGetSystemMetrics = user32.NewProc("GetSystemMetrics")
)

const (
	inputMouse    = 0
	inputKeyboard = 1

	mouseeventfMove        = 0x0001
	mouseeventfLeftdown    = 0x0002
	mouseeventfLeftup      = 0x0004
	mouseeventfRightdown   = 0x0008
	mouseeventfRightup     = 0x0010
	mouseeventfMiddledown  = 0x0020
	mouseeventfMiddleup    = 0x0040
	mouseeventfWheel       = 0x0800
	mouseeventfHwheel      = 0x1000
	mouseeventfAbsolute    = 0x8000
	mouseeventfVirtualdesk = 0x4000

	keyeventfExtendedkey = 0x0001
	keyeventfKeyup       = 0x0002

	smCxvirtualscreen = 78
	smCyvirtualscreen = 79
)

// mouseInput is the Windows INPUT structure for mouse events.
type mouseInput struct {
	inputType uint32
	mi        struct {
		dx          int32
		dy          int32
		mouseData   uint32
		dwFlags     uint32
		time        uint32
		dwExtraInfo uintptr
	}
	_pad [8]byte // align to 28 bytes (union size)
}

// keyboardInput is the Windows INPUT structure for keyboard events.
type keyboardInput struct {
	inputType uint32
	ki        struct {
		wVk         uint16
		wScan       uint16
		dwFlags     uint32
		time        uint32
		dwExtraInfo uintptr
	}
	_pad [8]byte
}

func injectInput(evt *InputEvent) error {
	switch evt.Type {
	case "mouse_move":
		return sendMouseMove(evt.X, evt.Y)

	case "mouse_click":
		if err := sendMouseMove(evt.X, evt.Y); err != nil {
			return err
		}
		return sendMouseClick(evt.Button, false)

	case "mouse_down":
		return sendMouseClick(evt.Button, false)

	case "mouse_up":
		return sendMouseClick(evt.Button, true)

	case "mouse_scroll":
		if evt.DeltaY != 0 {
			return sendMouseWheel(evt.DeltaY * 120)
		}
		if evt.DeltaX != 0 {
			return sendMouseHWheel(evt.DeltaX * 120)
		}
		return nil

	case "key_press":
		vk, ext := cdapKeyToVK(evt.Key)
		if vk == 0 {
			return nil
		}
		for _, mod := range evt.Modifiers {
			modVK := modifierVK(mod)
			if modVK != 0 {
				if err := sendKey(modVK, false, false); err != nil {
					return err
				}
			}
		}
		return sendKey(vk, ext, false)

	case "key_release":
		vk, ext := cdapKeyToVK(evt.Key)
		if vk == 0 {
			return nil
		}
		if err := sendKey(vk, ext, true); err != nil {
			return err
		}
		for _, mod := range evt.Modifiers {
			modVK := modifierVK(mod)
			if modVK != 0 {
				if err := sendKey(modVK, false, true); err != nil {
					return err
				}
			}
		}
		return nil

	case "key_tap":
		vk, ext := cdapKeyToVK(evt.Key)
		if vk == 0 {
			return nil
		}
		for _, mod := range evt.Modifiers {
			modVK := modifierVK(mod)
			if modVK != 0 {
				if err := sendKey(modVK, false, false); err != nil {
					return err
				}
			}
		}
		if err := sendKey(vk, ext, false); err != nil {
			return err
		}
		if err := sendKey(vk, ext, true); err != nil {
			return err
		}
		for i := len(evt.Modifiers) - 1; i >= 0; i-- {
			modVK := modifierVK(evt.Modifiers[i])
			if modVK != 0 {
				if err := sendKey(modVK, false, true); err != nil {
					return err
				}
			}
		}
		return nil

	case "text":
		return sendText(evt.Text)

	default:
		return fmt.Errorf("unknown input type: %s", evt.Type)
	}
}

func sendMouseMove(x, y int) error {
	sw, _, _ := procGetSystemMetrics.Call(uintptr(smCxvirtualscreen))
	sh, _, _ := procGetSystemMetrics.Call(uintptr(smCyvirtualscreen))
	if sw == 0 {
		sw = 1920
	}
	if sh == 0 {
		sh = 1080
	}

	// Normalise to 0–65535 range (MOUSEEVENTF_ABSOLUTE)
	dx := int32(x * 65535 / int(sw))
	dy := int32(y * 65535 / int(sh))

	inp := mouseInput{inputType: inputMouse}
	inp.mi.dx = dx
	inp.mi.dy = dy
	inp.mi.dwFlags = mouseeventfMove | mouseeventfAbsolute | mouseeventfVirtualdesk

	return callSendInput(unsafe.Pointer(&inp), unsafe.Sizeof(inp))
}

func sendMouseClick(button int, up bool) error {
	inp := mouseInput{inputType: inputMouse}
	switch button {
	case 1: // left
		if up {
			inp.mi.dwFlags = mouseeventfLeftup
		} else {
			inp.mi.dwFlags = mouseeventfLeftdown
		}
	case 2: // right
		if up {
			inp.mi.dwFlags = mouseeventfRightup
		} else {
			inp.mi.dwFlags = mouseeventfRightdown
		}
	case 3: // middle
		if up {
			inp.mi.dwFlags = mouseeventfMiddleup
		} else {
			inp.mi.dwFlags = mouseeventfMiddledown
		}
	}
	return callSendInput(unsafe.Pointer(&inp), unsafe.Sizeof(inp))
}

func sendMouseWheel(delta int) error {
	inp := mouseInput{inputType: inputMouse}
	inp.mi.dwFlags = mouseeventfWheel
	inp.mi.mouseData = uint32(int32(delta))
	return callSendInput(unsafe.Pointer(&inp), unsafe.Sizeof(inp))
}

func sendMouseHWheel(delta int) error {
	inp := mouseInput{inputType: inputMouse}
	inp.mi.dwFlags = mouseeventfHwheel
	inp.mi.mouseData = uint32(int32(delta))
	return callSendInput(unsafe.Pointer(&inp), unsafe.Sizeof(inp))
}

func sendKey(vk uint16, extended bool, keyUp bool) error {
	inp := keyboardInput{inputType: inputKeyboard}
	inp.ki.wVk = vk
	if extended {
		inp.ki.dwFlags |= keyeventfExtendedkey
	}
	if keyUp {
		inp.ki.dwFlags |= keyeventfKeyup
	}
	return callSendInput(unsafe.Pointer(&inp), unsafe.Sizeof(inp))
}

func sendText(text string) error {
	for _, r := range text {
		inp := keyboardInput{inputType: inputKeyboard}
		inp.ki.wVk = 0
		inp.ki.wScan = uint16(r)
		inp.ki.dwFlags = 0x0004 // KEYEVENTF_UNICODE
		if err := callSendInput(unsafe.Pointer(&inp), unsafe.Sizeof(inp)); err != nil {
			return err
		}
		inp.ki.dwFlags = 0x0004 | keyeventfKeyup
		if err := callSendInput(unsafe.Pointer(&inp), unsafe.Sizeof(inp)); err != nil {
			return err
		}
	}
	return nil
}

func callSendInput(inp unsafe.Pointer, size uintptr) error {
	ret, _, err := procSendInput.Call(1, uintptr(inp), size)
	if ret == 0 {
		return fmt.Errorf("SendInput failed: %w", err)
	}
	return nil
}

// Virtual key codes
var cdapKeyVKMap = map[string]uint16{
	"Return": 0x0D, "Enter": 0x0D,
	"Backspace": 0x08,
	"Delete":    0x2E, "Del": 0x2E,
	"Escape": 0x1B, "Esc": 0x1B,
	"Tab":   0x09,
	"Space": 0x20, " ": 0x20,
	"ArrowUp": 0x26, "Up": 0x26,
	"ArrowDown": 0x28, "Down": 0x28,
	"ArrowLeft": 0x25, "Left": 0x25,
	"ArrowRight": 0x27, "Right": 0x27,
	"Home": 0x24, "End": 0x23,
	"PageUp": 0x21, "PageDown": 0x22,
	"Insert": 0x2D,
	"F1":     0x70, "F2": 0x71, "F3": 0x72, "F4": 0x73,
	"F5": 0x74, "F6": 0x75, "F7": 0x76, "F8": 0x77,
	"F9": 0x78, "F10": 0x79, "F11": 0x7A, "F12": 0x7B,
	"PrintScreen": 0x2C,
	"CapsLock":    0x14, "NumLock": 0x90, "ScrollLock": 0x91,
}

// Extended keys (require KEYEVENTF_EXTENDEDKEY)
var extendedKeys = map[uint16]bool{
	0x26: true, 0x28: true, 0x25: true, 0x27: true, // arrows
	0x24: true, 0x23: true, 0x21: true, 0x22: true, // home/end/pgup/pgdn
	0x2D: true, 0x2E: true, // insert/delete
}

func cdapKeyToVK(key string) (uint16, bool) {
	if vk, ok := cdapKeyVKMap[key]; ok {
		return vk, extendedKeys[vk]
	}
	// Single printable character: use VkKeyScanA
	if len(key) == 1 {
		// Use the ASCII value directly for letters/digits
		ch := key[0]
		if ch >= 'a' && ch <= 'z' {
			return uint16(ch - 32), false // uppercase VK
		}
		if ch >= 'A' && ch <= 'Z' {
			return uint16(ch), false
		}
		if ch >= '0' && ch <= '9' {
			return uint16(ch), false
		}
	}
	return 0, false
}

func modifierVK(mod string) uint16 {
	switch strings.ToLower(mod) {
	case "ctrl", "control":
		return 0x11 // VK_CONTROL
	case "alt":
		return 0x12 // VK_MENU
	case "shift":
		return 0x10 // VK_SHIFT
	case "super", "meta", "win", "cmd":
		return 0x5B // VK_LWIN
	default:
		return 0
	}
}

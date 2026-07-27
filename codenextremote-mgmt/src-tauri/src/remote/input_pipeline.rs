//! Input forwarding pipeline — builds protobuf `MouseEvent` / `KeyEvent`
//! messages from frontend JS events and queues them for relay transmission.
//!
//! Mouse mask encoding matches RustDesk:
//!   `mask = type | (button << 3)`
//!   type: 1=down, 2=up, 3=wheel, 0x8000=move (no button)
//!   button: 1=left, 2=right, 4=middle

use log::debug;

use crate::proto::{
    ControlKey, KeyEvent, MouseEvent,
    key_event, message::Union as MsgUnion, Message as PeerMessage,
};

// ---------------------------------------------------------------------------
//  Mouse constants (match web client's input.js encoding)
// ---------------------------------------------------------------------------

const MOUSE_TYPE_DOWN: i32 = 1;
const MOUSE_TYPE_UP: i32 = 2;
const MOUSE_TYPE_WHEEL: i32 = 3;
const MOUSE_TYPE_MOVE: i32 = 0x8000;
const MOUSE_BUTTON_LEFT: i32 = 1;
const MOUSE_BUTTON_RIGHT: i32 = 2;
const MOUSE_BUTTON_MIDDLE: i32 = 4;

// ---------------------------------------------------------------------------
//  Public API
// ---------------------------------------------------------------------------

/// Build a protobuf `MouseEvent` for a mouse move.
pub fn build_mouse_move(x: i32, y: i32) -> PeerMessage {
    PeerMessage {
        union: Some(MsgUnion::MouseEvent(MouseEvent {
            mask: MOUSE_TYPE_MOVE,
            x,
            y,
            modifiers: Vec::new(),
        })),
    }
}

/// Build a protobuf `MouseEvent` for a button press/release.
///
/// `button`: 0=left, 1=middle, 2=right (JS button codes)
/// `down`: true=press, false=release
pub fn build_mouse_button(x: i32, y: i32, button: u8, down: bool, modifiers: &[String]) -> PeerMessage {
    let btn = match button {
        2 => MOUSE_BUTTON_RIGHT,
        1 => MOUSE_BUTTON_MIDDLE,
        _ => MOUSE_BUTTON_LEFT,
    };
    let kind = if down { MOUSE_TYPE_DOWN } else { MOUSE_TYPE_UP };
    let mask = kind | (btn << 3);

    PeerMessage {
        union: Some(MsgUnion::MouseEvent(MouseEvent {
            mask,
            x,
            y,
            modifiers: map_modifiers(modifiers),
        })),
    }
}

/// Build a protobuf `MouseEvent` for a scroll wheel.
///
/// `delta_y`: positive = scroll down, negative = scroll up
pub fn build_mouse_wheel(_x: i32, _y: i32, delta_x: i32, delta_y: i32) -> PeerMessage {
    // RustDesk encodes scroll delta in x/y fields with wheel type
    // The mask has wheel type, y holds the delta
    let mask = MOUSE_TYPE_WHEEL;
    PeerMessage {
        union: Some(MsgUnion::MouseEvent(MouseEvent {
            mask,
            x: delta_x,
            y: delta_y,
            modifiers: Vec::new(),
        })),
    }
}

/// Build a protobuf `KeyEvent` from a JS key name.
///
/// `key`: JS `KeyboardEvent.key` value (e.g. "a", "Enter", "Control")
/// `down`: true=press, false=release
pub fn build_key_event(key: &str, down: bool, modifiers: &[String]) -> Option<PeerMessage> {
    let mods = map_modifiers(modifiers);

    // Try to map to a ControlKey first
    if let Some(ck) = map_control_key(key) {
        let ke = KeyEvent {
            down,
            press: false,
            modifiers: mods,
            mode: 0, // Legacy mode
            union: Some(key_event::Union::ControlKey(ck as i32)),
        };
        return Some(PeerMessage {
            union: Some(MsgUnion::KeyEvent(ke)),
        });
    }

    // Single character → Unicode key event
    let chars: Vec<char> = key.chars().collect();
    if chars.len() == 1 {
        let ke = KeyEvent {
            down,
            press: false,
            modifiers: mods,
            mode: 0,
            union: Some(key_event::Union::Unicode(chars[0] as u32)),
        };
        return Some(PeerMessage {
            union: Some(MsgUnion::KeyEvent(ke)),
        });
    }

    debug!("Unmapped key: {}", key);
    None
}

/// Build a Ctrl+Alt+Del special key sequence.
pub fn build_ctrl_alt_del() -> PeerMessage {
    let ke = KeyEvent {
        down: true,
        press: true,
        modifiers: Vec::new(),
        mode: 0,
        union: Some(key_event::Union::ControlKey(ControlKey::CtrlAltDel as i32)),
    };
    PeerMessage {
        union: Some(MsgUnion::KeyEvent(ke)),
    }
}

/// Build a LockScreen special key.
pub fn build_lock_screen() -> PeerMessage {
    let ke = KeyEvent {
        down: true,
        press: true,
        modifiers: Vec::new(),
        mode: 0,
        union: Some(key_event::Union::ControlKey(ControlKey::LockScreen as i32)),
    };
    PeerMessage {
        union: Some(MsgUnion::KeyEvent(ke)),
    }
}

// ---------------------------------------------------------------------------
//  Key mapping
// ---------------------------------------------------------------------------

fn map_control_key(key: &str) -> Option<ControlKey> {
    Some(match key {
        "Enter" | "Return" => ControlKey::Return,
        "Escape" => ControlKey::Escape,
        "Tab" => ControlKey::Tab,
        "Backspace" => ControlKey::Backspace,
        "Delete" => ControlKey::Delete,
        "Insert" => ControlKey::Insert,
        "Home" => ControlKey::Home,
        "End" => ControlKey::End,
        "PageUp" => ControlKey::PageUp,
        "PageDown" => ControlKey::PageDown,
        "ArrowLeft" | "Left" => ControlKey::LeftArrow,
        "ArrowRight" | "Right" => ControlKey::RightArrow,
        "ArrowUp" | "Up" => ControlKey::UpArrow,
        "ArrowDown" | "Down" => ControlKey::DownArrow,
        "Control" | "ControlLeft" => ControlKey::Control,
        "ControlRight" => ControlKey::RControl,
        "Alt" | "AltLeft" => ControlKey::Alt,
        "AltRight" => ControlKey::RAlt,
        "Shift" | "ShiftLeft" => ControlKey::Shift,
        "ShiftRight" => ControlKey::RShift,
        "Meta" | "MetaLeft" | "OS" => ControlKey::Meta,
        "MetaRight" => ControlKey::RWin,
        "CapsLock" => ControlKey::CapsLock,
        "NumLock" => ControlKey::NumLock,
        "ScrollLock" => ControlKey::Scroll,
        "Pause" => ControlKey::Pause,
        "PrintScreen" => ControlKey::Snapshot,
        "ContextMenu" => ControlKey::Apps,
        "F1" => ControlKey::F1,
        "F2" => ControlKey::F2,
        "F3" => ControlKey::F3,
        "F4" => ControlKey::F4,
        "F5" => ControlKey::F5,
        "F6" => ControlKey::F6,
        "F7" => ControlKey::F7,
        "F8" => ControlKey::F8,
        "F9" => ControlKey::F9,
        "F10" => ControlKey::F10,
        "F11" => ControlKey::F11,
        "F12" => ControlKey::F12,
        _ => return None,
    })
}

fn map_modifiers(mods: &[String]) -> Vec<i32> {
    mods.iter()
        .filter_map(|m| {
            match m.as_str() {
                "Control" => Some(ControlKey::Control as i32),
                "Shift" => Some(ControlKey::Shift as i32),
                "Alt" => Some(ControlKey::Alt as i32),
                "Meta" => Some(ControlKey::Meta as i32),
                _ => None,
            }
        })
        .collect()
}

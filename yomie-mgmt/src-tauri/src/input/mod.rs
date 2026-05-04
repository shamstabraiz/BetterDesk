//! Cross-platform input simulation.
//!
//! Uses the `enigo` crate for keyboard and mouse input.
//! When running as a service with elevated privileges, this can simulate
//! input on secure desktops (UAC, login screen).

use anyhow::{Context, Result};
use enigo::{
    Direction::{Press, Release},
    Enigo, Key, Keyboard, Mouse, Settings,
};
use log::debug;
use std::sync::Mutex;

static ENIGO: Mutex<Option<Enigo>> = Mutex::new(None);

fn get_enigo() -> Result<std::sync::MutexGuard<'static, Option<Enigo>>> {
    let mut guard = ENIGO.lock().map_err(|e| anyhow::anyhow!("enigo mutex: {e}"))?;
    if guard.is_none() {
        let settings = Settings {
            ..Default::default()
        };
        *guard = Some(Enigo::new(&settings).context("failed to create Enigo instance")?);
    }
    Ok(guard)
}

/// Map a key string (from frontend/protocol) to enigo Key.
fn map_key(key: &str) -> Option<Key> {
    match key {
        "Return" | "Enter" => Some(Key::Return),
        "Tab" => Some(Key::Tab),
        "Escape" | "Esc" => Some(Key::Escape),
        "Backspace" => Some(Key::Backspace),
        "Delete" => Some(Key::Delete),
        "Home" => Some(Key::Home),
        "End" => Some(Key::End),
        "PageUp" => Some(Key::PageUp),
        "PageDown" => Some(Key::PageDown),
        "ArrowLeft" | "Left" => Some(Key::LeftArrow),
        "ArrowRight" | "Right" => Some(Key::RightArrow),
        "ArrowUp" | "Up" => Some(Key::UpArrow),
        "ArrowDown" | "Down" => Some(Key::DownArrow),
        "Space" | " " => Some(Key::Space),
        "CapsLock" => Some(Key::CapsLock),
        "F1" => Some(Key::F1),
        "F2" => Some(Key::F2),
        "F3" => Some(Key::F3),
        "F4" => Some(Key::F4),
        "F5" => Some(Key::F5),
        "F6" => Some(Key::F6),
        "F7" => Some(Key::F7),
        "F8" => Some(Key::F8),
        "F9" => Some(Key::F9),
        "F10" => Some(Key::F10),
        "F11" => Some(Key::F11),
        "F12" => Some(Key::F12),
        "Shift" | "ShiftLeft" | "ShiftRight" => Some(Key::Shift),
        "Control" | "ControlLeft" | "ControlRight" => Some(Key::Control),
        "Alt" | "AltLeft" | "AltRight" => Some(Key::Alt),
        "Meta" | "MetaLeft" | "MetaRight" => Some(Key::Meta),
        s if s.len() == 1 => {
            let ch = s.chars().next()?;
            Some(Key::Unicode(ch))
        }
        _ => {
            debug!("Unknown key: {}", key);
            None
        }
    }
}

/// Simulate a key press or release.
pub fn simulate_key(key: &str, down: bool, modifiers: &[String]) -> Result<()> {
    debug!(
        "Simulate key: {} down={} modifiers={:?}",
        key, down, modifiers
    );

    let mapped = match map_key(key) {
        Some(k) => k,
        None => return Ok(()),
    };

    let mut guard = get_enigo()?;
    let enigo = guard.as_mut().context("enigo not initialized")?;

    // Press modifier keys
    if down {
        for m in modifiers {
            if let Some(mk) = map_key(m) {
                let _ = enigo.key(mk, Press);
            }
        }
    }

    // Press or release the main key
    let direction = if down { Press } else { Release };
    enigo
        .key(mapped, direction)
        .map_err(|e| anyhow::anyhow!("enigo key error: {e:?}"))?;

    // Release modifier keys (only on key-up to allow combos)
    if !down {
        for m in modifiers.iter().rev() {
            if let Some(mk) = map_key(m) {
                let _ = enigo.key(mk, Release);
            }
        }
    }

    Ok(())
}

/// Simulate a mouse move and/or click.
pub fn simulate_mouse(x: i32, y: i32, mask: u32, _modifiers: &[String]) -> Result<()> {
    debug!("Simulate mouse: ({}, {}) mask={:#06x}", x, y, mask);

    let mut guard = get_enigo()?;
    let enigo = guard.as_mut().context("enigo not initialized")?;

    // Move mouse to absolute position
    if mask & mask_bits::MOVE != 0 || mask == 0 {
        enigo
            .move_mouse(x, y, enigo::Coordinate::Abs)
            .map_err(|e| anyhow::anyhow!("mouse move error: {e:?}"))?;
    }

    // Button events
    if mask & mask_bits::LEFT_DOWN != 0 {
        enigo
            .button(enigo::Button::Left, Press)
            .map_err(|e| anyhow::anyhow!("mouse btn error: {e:?}"))?;
    }
    if mask & mask_bits::LEFT_UP != 0 {
        enigo
            .button(enigo::Button::Left, Release)
            .map_err(|e| anyhow::anyhow!("mouse btn error: {e:?}"))?;
    }
    if mask & mask_bits::RIGHT_DOWN != 0 {
        enigo
            .button(enigo::Button::Right, Press)
            .map_err(|e| anyhow::anyhow!("mouse btn error: {e:?}"))?;
    }
    if mask & mask_bits::RIGHT_UP != 0 {
        enigo
            .button(enigo::Button::Right, Release)
            .map_err(|e| anyhow::anyhow!("mouse btn error: {e:?}"))?;
    }
    if mask & mask_bits::MIDDLE_DOWN != 0 {
        enigo
            .button(enigo::Button::Middle, Press)
            .map_err(|e| anyhow::anyhow!("mouse btn error: {e:?}"))?;
    }
    if mask & mask_bits::MIDDLE_UP != 0 {
        enigo
            .button(enigo::Button::Middle, Release)
            .map_err(|e| anyhow::anyhow!("mouse btn error: {e:?}"))?;
    }

    // Scroll wheel
    if mask & mask_bits::WHEEL != 0 {
        // y > 0 = scroll up, y < 0 = scroll down (delta encoded in y when WHEEL is set)
        let delta = if y > 0 { 3 } else { -3 };
        enigo
            .scroll(delta, enigo::Axis::Vertical)
            .map_err(|e| anyhow::anyhow!("scroll error: {e:?}"))?;
    }

    Ok(())
}

/// Simulate typing a full text string.
pub fn simulate_text(text: &str) -> Result<()> {
    debug!("Simulate text: {:?}", text);

    let mut guard = get_enigo()?;
    let enigo = guard.as_mut().context("enigo not initialized")?;

    enigo
        .text(text)
        .map_err(|e| anyhow::anyhow!("enigo text error: {e:?}"))?;

    Ok(())
}

/// Mouse button mask constants (matching RustDesk protocol).
pub mod mask_bits {
    pub const LEFT_DOWN: u32 = 0x01;
    pub const LEFT_UP: u32 = 0x02;
    pub const RIGHT_DOWN: u32 = 0x04;
    pub const RIGHT_UP: u32 = 0x08;
    pub const MIDDLE_DOWN: u32 = 0x10;
    pub const MIDDLE_UP: u32 = 0x20;
    pub const MOVE: u32 = 0x8000;
    pub const WHEEL: u32 = 0x0080;
}

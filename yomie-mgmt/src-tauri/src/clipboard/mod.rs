//! Cross-platform clipboard synchronization.
//!
//! Uses the `arboard` crate for clipboard access.
//! Supports text, RTF, HTML, and image formats.
//!
//! TODO: Implement clipboard monitoring and sync with peer.

use anyhow::{Context, Result};
use arboard::Clipboard;
use log::debug;

/// Get the current clipboard text content.
pub fn get_text() -> Result<String> {
    let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
    clipboard.get_text().context("Failed to get clipboard text")
}

/// Set the clipboard text content.
pub fn set_text(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
    clipboard
        .set_text(text)
        .context("Failed to set clipboard text")?;
    debug!("Clipboard set: {} chars", text.len());
    Ok(())
}

/// Get the current clipboard image as RGBA data.
pub fn get_image() -> Result<(Vec<u8>, usize, usize)> {
    let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
    let img = clipboard
        .get_image()
        .context("Failed to get clipboard image")?;
    Ok((
        img.bytes.into_owned(),
        img.width,
        img.height,
    ))
}

/// Set the clipboard image from RGBA data.
pub fn set_image(data: &[u8], width: usize, height: usize) -> Result<()> {
    let mut clipboard = Clipboard::new().context("Failed to access clipboard")?;
    let img = arboard::ImageData {
        width,
        height,
        bytes: std::borrow::Cow::Borrowed(data),
    };
    clipboard
        .set_image(img)
        .context("Failed to set clipboard image")?;
    debug!("Clipboard image set: {}x{}", width, height);
    Ok(())
}

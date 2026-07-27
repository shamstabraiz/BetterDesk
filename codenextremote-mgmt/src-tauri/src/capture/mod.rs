//! Cross-platform screen capture.
//!
//! Uses the `scrap` crate:
//! - Windows: DXGI Desktop Duplication API
//! - Linux: X11 shared memory
//! - macOS: CGDisplay stream
//!
//! TODO: Implement actual frame capture and encoding pipeline.

use anyhow::Result;

/// Display information.
#[derive(Debug, Clone)]
pub struct DisplayInfo {
    pub index: usize,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

/// List available displays.
pub fn list_displays() -> Result<Vec<DisplayInfo>> {
    let displays = scrap::Display::all()
        .map_err(|e| anyhow::anyhow!("Failed to enumerate displays: {}", e))?;

    let result: Vec<DisplayInfo> = displays
        .iter()
        .enumerate()
        .map(|(i, d)| DisplayInfo {
            index: i,
            name: format!("Display {}", i),
            width: d.width() as u32,
            height: d.height() as u32,
            is_primary: i == 0,
        })
        .collect();

    Ok(result)
}

/// Capture a single frame from the specified display.
///
/// Returns raw BGRA pixel data.
pub fn capture_frame(display_index: usize) -> Result<(Vec<u8>, u32, u32)> {
    let displays = scrap::Display::all()
        .map_err(|e| anyhow::anyhow!("Failed to enumerate displays: {}", e))?;

    let display = displays
        .into_iter()
        .nth(display_index)
        .ok_or_else(|| anyhow::anyhow!("Display {} not found", display_index))?;

    let width = display.width() as u32;
    let height = display.height() as u32;

    let mut capturer = scrap::Capturer::new(display)
        .map_err(|e| anyhow::anyhow!("Failed to create capturer: {}", e))?;

    // Try to capture (may need multiple attempts due to frame timing)
    for _ in 0..10 {
        match capturer.frame() {
            Ok(frame) => {
                return Ok((frame.to_vec(), width, height));
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    continue;
                }
                return Err(anyhow::anyhow!("Capture failed: {}", e));
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed to capture frame after 10 attempts"
    ))
}

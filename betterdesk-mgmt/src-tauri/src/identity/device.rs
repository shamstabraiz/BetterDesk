//! Device ID generation and persistence.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

/// Get or create a persistent 9-digit device ID.
///
/// Stored at `<config_dir>/yomie/id`.
pub fn get_or_create_device_id() -> Result<String> {
    let id_path = id_file_path()?;

    // Try to load existing ID
    if id_path.exists() {
        let id = fs::read_to_string(&id_path)
            .context("Failed to read device ID file")?
            .trim()
            .to_string();
        if is_valid_id(&id) {
            return Ok(id);
        }
        log::warn!("Stored device ID is invalid ({}), regenerating", id);
    }

    // Generate a new ID
    let id = generate_device_id();
    log::info!("Generated new device ID: {}", id);

    // Persist
    if let Some(parent) = id_path.parent() {
        fs::create_dir_all(parent).context("Failed to create config directory")?;
    }
    fs::write(&id_path, &id).context("Failed to write device ID file")?;

    Ok(id)
}

/// Generate a 9-digit numeric ID from machine fingerprint.
fn generate_device_id() -> String {
    let fingerprint = machine_fingerprint();
    let hash = Sha256::digest(fingerprint.as_bytes());

    // Extract 9 digits from the hash
    let mut num: u64 = 0;
    for &b in &hash[..8] {
        num = (num << 8) | b as u64;
    }

    // Ensure it's exactly 9 digits (100_000_000 .. 999_999_999)
    let id = 100_000_000 + (num % 900_000_000);
    format!("{}", id)
}

/// Build a machine-specific fingerprint string.
fn machine_fingerprint() -> String {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".into());

    let username = whoami::username();
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    // Include a random component if we can't get a stable fingerprint
    let random_seed = if let Ok(id) = machine_uid::get() {
        id
    } else {
        uuid::Uuid::new_v4().to_string()
    };

    format!(
        "yomie:{}:{}:{}:{}:{}",
        hostname, username, os, arch, random_seed
    )
}

/// Validate a device ID (9 digits, no leading zero after the first digit).
fn is_valid_id(id: &str) -> bool {
    id.len() == 9 && id.chars().all(|c| c.is_ascii_digit()) && !id.starts_with('0')
}

/// Path to the device ID file.
fn id_file_path() -> Result<PathBuf> {
    let config_dir = directories::ProjectDirs::from("com", "yomie", "Yomie")
        .context("Failed to determine config directory")?
        .config_dir()
        .to_path_buf();
    Ok(config_dir.join("id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_id_is_valid() {
        let id = generate_device_id();
        assert!(is_valid_id(&id), "Generated ID is invalid: {}", id);
    }

    #[test]
    fn id_is_9_digits() {
        let id = generate_device_id();
        assert_eq!(id.len(), 9);
        assert!(id.parse::<u64>().is_ok());
    }
}

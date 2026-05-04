//! File browser for CDAP — list, read, write, delete with path traversal
//! protection.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use std::fs;
use std::path::PathBuf;

use super::protocol::FileEntry;

/// Maximum file chunk size: 1 MB.
const MAX_CHUNK: u64 = 1_048_576;

// ---------------------------------------------------------------------------
//  Public API
// ---------------------------------------------------------------------------

/// List directory entries.
pub fn list_directory(root: &str, relative_path: &str) -> Result<Vec<FileEntry>> {
    let safe_path = safe_path(root, relative_path)?;

    if !safe_path.is_dir() {
        bail!("Not a directory: {}", relative_path);
    }

    let mut entries = Vec::new();
    let read_dir = fs::read_dir(&safe_path).context("Read directory")?;

    for entry in read_dir {
        let entry = entry?;
        let metadata = entry.metadata().unwrap_or_else(|_| {
            // Fallback for symlinks etc.
            fs::metadata(entry.path()).unwrap_or_else(|_| entry.metadata().unwrap())
        });

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_secs())
            })
            .map(|secs| {
                chrono::DateTime::from_timestamp(secs as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        entries.push(FileEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            is_dir: metadata.is_dir(),
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
            modified,
            permissions: None,
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

/// Read a file chunk as base64.
pub fn read_file_chunk(
    root: &str,
    relative_path: &str,
    offset: u64,
    length: u64,
) -> Result<String> {
    let safe_path = safe_path(root, relative_path)?;

    if !safe_path.is_file() {
        bail!("Not a file: {}", relative_path);
    }

    let chunk_size = length.min(MAX_CHUNK) as usize;

    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(&safe_path).context("Open file")?;
    file.seek(SeekFrom::Start(offset))?;

    let mut buf = vec![0u8; chunk_size];
    let n = file.read(&mut buf)?;
    buf.truncate(n);

    Ok(B64.encode(&buf))
}

/// Write a base64-encoded chunk to a file.
pub fn write_file_chunk(
    root: &str,
    relative_path: &str,
    data_b64: &str,
    offset: u64,
) -> Result<()> {
    let safe_path = safe_path(root, relative_path)?;

    // Ensure parent directory exists
    if let Some(parent) = safe_path.parent() {
        fs::create_dir_all(parent).context("Create parent directory")?;
    }

    let data = B64.decode(data_b64).context("Decode base64")?;

    if data.len() > MAX_CHUNK as usize {
        bail!("Chunk too large: {} bytes", data.len());
    }

    use std::io::{Seek, SeekFrom, Write};
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&safe_path)
        .context("Open file for writing")?;

    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&data)?;

    Ok(())
}

/// Delete a file or directory.
pub fn delete_path(root: &str, relative_path: &str) -> Result<()> {
    let safe_path = safe_path(root, relative_path)?;

    if safe_path.is_dir() {
        fs::remove_dir_all(&safe_path).context("Remove directory")?;
    } else if safe_path.is_file() {
        fs::remove_file(&safe_path).context("Remove file")?;
    } else {
        bail!("Path not found: {}", relative_path);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
//  Security
// ---------------------------------------------------------------------------

/// Validate and resolve a path, preventing directory traversal.
///
/// Returns the canonical absolute path, ensuring it stays within `root`.
fn safe_path(root: &str, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() || relative == "/" || relative == "\\" {
        return Ok(PathBuf::from(root));
    }

    // Reject obvious traversal attempts
    let normalized = relative.replace('\\', "/");
    if normalized.contains("..") {
        bail!("Path traversal detected: {}", relative);
    }

    let root_path = PathBuf::from(root);
    let candidate = root_path.join(&normalized);

    // Canonicalize and verify it's under root
    let canon_root = fs::canonicalize(&root_path).unwrap_or_else(|_| root_path.clone());
    let canon_candidate = if candidate.exists() {
        fs::canonicalize(&candidate).context("Canonicalize path")?
    } else {
        // For new files: canonicalize parent, append filename
        let parent = candidate
            .parent()
            .context("No parent directory")?;
        let parent_canon = fs::canonicalize(parent)
            .unwrap_or_else(|_| parent.to_path_buf());
        let filename = candidate
            .file_name()
            .context("No filename")?;
        parent_canon.join(filename)
    };

    if !canon_candidate.starts_with(&canon_root) {
        bail!(
            "Path escapes root: {} is outside {}",
            canon_candidate.display(),
            canon_root.display()
        );
    }

    Ok(canon_candidate)
}

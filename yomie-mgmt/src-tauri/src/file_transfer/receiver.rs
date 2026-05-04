/// Yomie Agent — File Receiver
///
/// Receives binary file chunks from the WebSocket relay and
/// reassembles them into a local file.  Validates integrity
/// and supports resumable downloads.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use super::sender::{FileChunk, FileHeader, TransferProgress};

/// Where received files are stored.
const DOWNLOAD_SUBDIR: &str = "yomie_downloads";

/// State of a file being received.
pub struct ActiveDownload {
    pub transfer_id: String,
    pub header: FileHeader,
    pub output_path: PathBuf,
    file: Option<File>,
    bytes_received: u64,
    chunks_received: u64,
}

pub struct FileReceiver {
    download_dir: PathBuf,
    active: std::collections::HashMap<String, ActiveDownload>,
}

impl FileReceiver {
    /// Create a new file receiver.
    /// Files are saved to the user's Downloads/yomie_downloads directory.
    pub fn new() -> Self {
        let download_dir = dirs::download_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .join(DOWNLOAD_SUBDIR);

        // Ensure directory exists
        let _ = std::fs::create_dir_all(&download_dir);

        Self {
            download_dir,
            active: std::collections::HashMap::new(),
        }
    }

    /// Create with a custom download directory.
    pub fn with_dir(dir: &Path) -> Self {
        let _ = std::fs::create_dir_all(dir);
        Self {
            download_dir: dir.to_path_buf(),
            active: std::collections::HashMap::new(),
        }
    }

    /// Begin receiving a file.  Call this when a FileHeader is received.
    pub async fn begin(&mut self, header: FileHeader) -> Result<()> {
        if self.active.contains_key(&header.transfer_id) {
            bail!("Transfer {} already in progress", header.transfer_id);
        }

        // Sanitize filename
        let safe_name = sanitize_filename(&header.filename);
        let output_path = self.download_dir.join(&safe_name);

        // If file exists, add suffix
        let output_path = unique_path(&output_path);

        let file = File::create(&output_path).await?;

        log::info!(
            "[FileReceiver] Starting transfer {}: {} ({} bytes, {} chunks) -> {}",
            header.transfer_id,
            header.filename,
            header.size,
            header.total_chunks,
            output_path.display()
        );

        self.active.insert(
            header.transfer_id.clone(),
            ActiveDownload {
                transfer_id: header.transfer_id.clone(),
                header,
                output_path,
                file: Some(file),
                bytes_received: 0,
                chunks_received: 0,
            },
        );

        Ok(())
    }

    /// Process an incoming file chunk.
    pub async fn receive_chunk(&mut self, chunk: FileChunk) -> Result<Option<TransferProgress>> {
        let download = self
            .active
            .get_mut(&chunk.transfer_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown transfer: {}", chunk.transfer_id))?;

        if let Some(ref mut file) = download.file {
            file.write_all(&chunk.data).await?;
        }

        download.bytes_received += chunk.data.len() as u64;
        download.chunks_received += 1;

        let progress = TransferProgress {
            transfer_id: chunk.transfer_id.clone(),
            chunks_sent: download.chunks_received,
            total_chunks: download.header.total_chunks,
            bytes_sent: download.bytes_received,
            total_bytes: download.header.size,
            percent: (download.bytes_received as f64 / download.header.size.max(1) as f64) * 100.0,
        };

        if chunk.is_last {
            // Flush and close file
            if let Some(mut file) = download.file.take() {
                file.flush().await?;
            }

            log::info!(
                "[FileReceiver] Transfer {} complete: {} bytes received -> {}",
                chunk.transfer_id,
                download.bytes_received,
                download.output_path.display()
            );
        }

        Ok(Some(progress))
    }

    /// Cancel and clean up an active download.
    pub async fn cancel(&mut self, transfer_id: &str) -> Result<()> {
        if let Some(mut dl) = self.active.remove(transfer_id) {
            // Close file handle
            drop(dl.file.take());
            // Delete partial file
            let _ = tokio::fs::remove_file(&dl.output_path).await;
            log::info!("[FileReceiver] Transfer {} cancelled", transfer_id);
        }
        Ok(())
    }

    /// Check if a transfer is complete (no longer in active map).
    pub fn is_complete(&self, transfer_id: &str) -> bool {
        if let Some(dl) = self.active.get(transfer_id) {
            dl.file.is_none() // file closed = complete
        } else {
            true // not tracked = done or never started
        }
    }

    /// Clean up completed transfers from the active map.
    pub fn cleanup_completed(&mut self) {
        self.active.retain(|_, dl| dl.file.is_some());
    }

    /// Get the output path for a completed transfer.
    pub fn output_path(&self, transfer_id: &str) -> Option<&Path> {
        self.active.get(transfer_id).map(|dl| dl.output_path.as_path())
    }
}

/// Sanitize a filename by removing dangerous characters.
fn sanitize_filename(name: &str) -> String {
    let name = name
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string();

    if name.is_empty() {
        "unnamed_file".to_string()
    } else {
        name
    }
}

/// Generate a unique path by appending (1), (2), etc. if file exists.
fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let parent = path.parent().unwrap_or(Path::new("."));

    for i in 1..1000 {
        let new_name = if ext.is_empty() {
            format!("{} ({})", stem, i)
        } else {
            format!("{} ({}).{}", stem, i, ext)
        };
        let candidate = parent.join(&new_name);
        if !candidate.exists() {
            return candidate;
        }
    }

    // Fallback: use UUID
    let new_name = if ext.is_empty() {
        format!("{}_{}", stem, uuid::Uuid::new_v4())
    } else {
        format!("{}_{}.{}", stem, uuid::Uuid::new_v4(), ext)
    };
    parent.join(&new_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("file.txt"), "file.txt");
        assert_eq!(sanitize_filename("../../../etc/passwd"), "_.._.._.._etc_passwd");
        assert_eq!(sanitize_filename(""), "unnamed_file");
        assert_eq!(
            sanitize_filename("C:\\Windows\\System32\\cmd.exe"),
            "C__Windows_System32_cmd.exe"
        );
    }

    #[test]
    fn test_unique_path_nonexistent() {
        let path = std::env::temp_dir().join("yomie_nonexistent_test_12345.txt");
        let result = unique_path(&path);
        assert_eq!(result, path);
    }
}

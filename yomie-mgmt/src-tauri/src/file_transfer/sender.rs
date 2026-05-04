/// Yomie Agent — File Sender
///
/// Reads a local file and streams it as binary frames over the
/// WebSocket relay to the remote peer.  Supports chunked transfer,
/// progress reporting, and cancellation.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// Default chunk size: 64 KB.
const CHUNK_SIZE: usize = 64 * 1024;

/// Maximum file size: 4 GB.
const MAX_FILE_SIZE: u64 = 4 * 1024 * 1024 * 1024;

/// File transfer header sent before data chunks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHeader {
    pub transfer_id: String,
    pub filename: String,
    pub size: u64,
    pub mime_type: String,
    pub chunk_size: usize,
    pub total_chunks: u64,
}

/// Progress of an ongoing transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub transfer_id: String,
    pub chunks_sent: u64,
    pub total_chunks: u64,
    pub bytes_sent: u64,
    pub total_bytes: u64,
    pub percent: f64,
}

/// A file chunk with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChunk {
    pub transfer_id: String,
    pub index: u64,
    pub data: Vec<u8>,
    pub is_last: bool,
}

pub struct FileSender {
    transfer_id: String,
    path: PathBuf,
    chunk_size: usize,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl FileSender {
    /// Create a new file sender for the given path.
    pub fn new(path: &Path) -> Result<Self> {
        if !path.exists() {
            bail!("File does not exist: {}", path.display());
        }
        let metadata = std::fs::metadata(path)?;
        if !metadata.is_file() {
            bail!("Not a regular file: {}", path.display());
        }
        if metadata.len() > MAX_FILE_SIZE {
            bail!(
                "File too large: {} bytes (max {})",
                metadata.len(),
                MAX_FILE_SIZE
            );
        }

        Ok(Self {
            transfer_id: uuid::Uuid::new_v4().to_string(),
            path: path.to_path_buf(),
            chunk_size: CHUNK_SIZE,
            cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    }

    /// Get the file header for this transfer.
    pub fn header(&self) -> Result<FileHeader> {
        let metadata = std::fs::metadata(&self.path)?;
        let size = metadata.len();
        let total_chunks = (size + self.chunk_size as u64 - 1) / self.chunk_size as u64;
        let filename = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let mime_type = mime_guess::from_path(&self.path)
            .first_or_octet_stream()
            .to_string();

        Ok(FileHeader {
            transfer_id: self.transfer_id.clone(),
            filename,
            size,
            mime_type,
            chunk_size: self.chunk_size,
            total_chunks,
        })
    }

    /// Read the file and yield chunks via the provided callback.
    /// The callback should send the chunk over the WebSocket.
    pub async fn stream_chunks<F, Fut>(
        &self,
        mut on_chunk: F,
        mut on_progress: Option<Box<dyn FnMut(TransferProgress) + Send>>,
    ) -> Result<()>
    where
        F: FnMut(FileChunk) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let header = self.header()?;
        let mut file = File::open(&self.path).await?;
        let mut buf = vec![0u8; self.chunk_size];
        let mut index: u64 = 0;
        let mut bytes_sent: u64 = 0;

        loop {
            if self
                .cancelled
                .load(std::sync::atomic::Ordering::Relaxed)
            {
                bail!("Transfer cancelled");
            }

            let n = file.read(&mut buf).await?;
            if n == 0 {
                break;
            }

            let is_last = bytes_sent + n as u64 >= header.size;
            let chunk = FileChunk {
                transfer_id: self.transfer_id.clone(),
                index,
                data: buf[..n].to_vec(),
                is_last,
            };

            on_chunk(chunk).await?;

            bytes_sent += n as u64;
            index += 1;

            if let Some(ref mut cb) = on_progress {
                cb(TransferProgress {
                    transfer_id: self.transfer_id.clone(),
                    chunks_sent: index,
                    total_chunks: header.total_chunks,
                    bytes_sent,
                    total_bytes: header.size,
                    percent: (bytes_sent as f64 / header.size as f64) * 100.0,
                });
            }

            if is_last {
                break;
            }
        }

        log::info!(
            "[FileSender] Transfer {} complete: {} bytes in {} chunks",
            self.transfer_id,
            bytes_sent,
            index
        );
        Ok(())
    }

    /// Cancel this transfer.
    pub fn cancel(&self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get the transfer ID.
    pub fn transfer_id(&self) -> &str {
        &self.transfer_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_header_generation() {
        let dir = std::env::temp_dir();
        let path = dir.join("betterdesk_test_sender.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"Hello, Yomie!").unwrap();
        drop(f);

        let sender = FileSender::new(&path).unwrap();
        let header = sender.header().unwrap();
        assert_eq!(header.size, 18);
        assert_eq!(header.filename, "betterdesk_test_sender.txt");
        assert_eq!(header.total_chunks, 1);

        std::fs::remove_file(&path).ok();
    }
}

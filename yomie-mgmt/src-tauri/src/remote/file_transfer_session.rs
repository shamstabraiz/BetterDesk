//! In-session file transfer — sends `FileAction` protobuf messages and
//! processes `FileResponse` messages from the relay connection.
//!
//! Supports:
//! - Remote directory browsing (ReadDir → FileDirectory)
//! - File download (Receive → Block stream → local file)
//! - Upload (Send → read local → Block stream → Done)
//! - Transfer cancel

use anyhow::{bail, Result};
use log::{debug, info, warn};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};

use crate::proto::{
    FileAction, FileTransferBlock, FileTransferCancel, FileTransferDone, FileTransferError,
    FileDirectory, ReadDir, FileTransferSendRequest, FileTransferReceiveRequest,
    file_action, file_response, FileResponse,
    message::Union as MsgUnion, Message as PeerMessage,
};

// ---------------------------------------------------------------------------
//  Types
// ---------------------------------------------------------------------------

/// Transfer direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransferDirection {
    Download,
    Upload,
}

/// Progress event emitted to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct TransferProgressEvent {
    pub id: i32,
    pub direction: String,
    pub filename: String,
    pub bytes_transferred: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub completed: bool,
    pub error: Option<String>,
}

/// Remote file entry (simplified for frontend).
#[derive(Debug, Clone, Serialize)]
pub struct RemoteFileEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_hidden: bool,
    pub size: u64,
    pub modified_time: u64,
}

/// Remote directory listing result.
#[derive(Debug, Clone, Serialize)]
pub struct RemoteDirResult {
    pub id: i32,
    pub path: String,
    pub entries: Vec<RemoteFileEntry>,
}

struct ActiveTransfer {
    _id: i32,
    direction: TransferDirection,
    filename: String,
    local_path: PathBuf,
    total_bytes: u64,
    bytes_transferred: u64,
    file_data: Vec<u8>,
    completed: bool,
    error: Option<String>,
}

// ---------------------------------------------------------------------------
//  FileTransferSession
// ---------------------------------------------------------------------------

/// Manages file transfers within an active remote session.
pub struct FileTransferSession {
    next_id: AtomicI32,
    active: HashMap<i32, ActiveTransfer>,
    download_dir: PathBuf,
    pending_dirs: HashMap<i32, String>,
    dir_results: Vec<RemoteDirResult>,
}

impl FileTransferSession {
    pub fn new() -> Self {
        let download_dir = dirs::download_dir()
            .unwrap_or_else(|| std::env::temp_dir())
            .join("betterdesk_transfers");
        let _ = std::fs::create_dir_all(&download_dir);

        Self {
            next_id: AtomicI32::new(1),
            active: HashMap::new(),
            download_dir,
            pending_dirs: HashMap::new(),
            dir_results: Vec::new(),
        }
    }

    /// Request a remote directory listing.
    pub fn browse_remote(&mut self, path: &str) -> PeerMessage {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.pending_dirs.insert(id, path.to_string());

        debug!("FileTransfer: browse remote dir {} (id={})", path, id);
        PeerMessage {
            union: Some(MsgUnion::FileAction(FileAction {
                union: Some(file_action::Union::ReadDir(ReadDir {
                    path: path.to_string(),
                    include_hidden: true,
                })),
            })),
        }
    }

    /// Start downloading a remote file.
    pub fn download(&mut self, remote_path: &str, filename: &str) -> PeerMessage {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        self.active.insert(id, ActiveTransfer {
            _id: id,
            direction: TransferDirection::Download,
            filename: filename.to_string(),
            local_path: self.download_dir.join(filename),
            total_bytes: 0,
            bytes_transferred: 0,
            file_data: Vec::new(),
            completed: false,
            error: None,
        });

        info!("FileTransfer: start download {} (id={})", remote_path, id);
        PeerMessage {
            union: Some(MsgUnion::FileAction(FileAction {
                union: Some(file_action::Union::Receive(FileTransferReceiveRequest {
                    id,
                    path: remote_path.to_string(),
                    files: Vec::new(),
                    file_num: 0,
                    total_size: 0,
                })),
            })),
        }
    }

    /// Start uploading a local file.
    pub fn upload(&mut self, local_path: &Path, remote_path: &str) -> Result<PeerMessage> {
        if !local_path.exists() {
            bail!("Local file not found: {}", local_path.display());
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let filename = local_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "upload".into());

        let metadata = std::fs::metadata(local_path)?;

        self.active.insert(id, ActiveTransfer {
            _id: id,
            direction: TransferDirection::Upload,
            filename: filename.clone(),
            local_path: local_path.to_path_buf(),
            total_bytes: metadata.len(),
            bytes_transferred: 0,
            file_data: Vec::new(),
            completed: false,
            error: None,
        });

        info!("FileTransfer: start upload {} → {} (id={})", local_path.display(), remote_path, id);
        Ok(PeerMessage {
            union: Some(MsgUnion::FileAction(FileAction {
                union: Some(file_action::Union::Send(FileTransferSendRequest {
                    id,
                    path: remote_path.to_string(),
                    include_hidden: false,
                    file_num: 0,
                })),
            })),
        })
    }

    /// Cancel an active transfer.
    pub fn cancel(&mut self, id: i32) -> PeerMessage {
        if let Some(t) = self.active.get_mut(&id) {
            t.error = Some("Cancelled".into());
            t.completed = true;
        }

        PeerMessage {
            union: Some(MsgUnion::FileAction(FileAction {
                union: Some(file_action::Union::Cancel(FileTransferCancel { id })),
            })),
        }
    }

    /// Handle a `FileResponse` from the peer.
    ///
    /// Returns a progress event if a transfer was updated.
    pub fn handle_response(&mut self, resp: &FileResponse) -> Option<TransferProgressEvent> {
        match &resp.union {
            Some(file_response::Union::Dir(dir)) => {
                self.handle_dir_response(dir);
                None
            }
            Some(file_response::Union::Block(block)) => {
                self.handle_block(block)
            }
            Some(file_response::Union::Done(done)) => {
                self.handle_done(done)
            }
            Some(file_response::Union::Error(err)) => {
                self.handle_error(err)
            }
            Some(file_response::Union::Digest(_digest)) => {
                // Digest is used for resume — acknowledge and continue
                None
            }
            None => None,
        }
    }

    /// Take any pending directory results.
    pub fn take_dir_results(&mut self) -> Vec<RemoteDirResult> {
        std::mem::take(&mut self.dir_results)
    }

    // -----------------------------------------------------------------------
    //  Internal handlers
    // -----------------------------------------------------------------------

    fn handle_dir_response(&mut self, dir: &FileDirectory) {
        let entries: Vec<RemoteFileEntry> = dir.entries.iter().map(|e| {
            RemoteFileEntry {
                name: e.name.clone(),
                is_dir: e.entry_type == 0 || e.entry_type == 2 || e.entry_type == 3, // Dir, DirLink, DirDrive
                is_hidden: e.is_hidden,
                size: e.size,
                modified_time: e.modified_time,
            }
        }).collect();

        debug!(
            "FileTransfer: dir response for {} ({} entries)",
            dir.path,
            entries.len()
        );

        self.dir_results.push(RemoteDirResult {
            id: dir.id,
            path: dir.path.clone(),
            entries,
        });
    }

    fn handle_block(&mut self, block: &FileTransferBlock) -> Option<TransferProgressEvent> {
        let transfer = self.active.get_mut(&block.id)?;
        transfer.bytes_transferred += block.data.len() as u64;
        transfer.file_data.extend_from_slice(&block.data);

        let percent = if transfer.total_bytes > 0 {
            (transfer.bytes_transferred as f64 / transfer.total_bytes as f64) * 100.0
        } else {
            0.0
        };

        Some(TransferProgressEvent {
            id: block.id,
            direction: format!("{:?}", transfer.direction),
            filename: transfer.filename.clone(),
            bytes_transferred: transfer.bytes_transferred,
            total_bytes: transfer.total_bytes,
            percent,
            completed: false,
            error: None,
        })
    }

    fn handle_done(&mut self, done: &FileTransferDone) -> Option<TransferProgressEvent> {
        let transfer = self.active.get_mut(&done.id)?;
        transfer.completed = true;

        // Write downloaded file to disk
        if transfer.direction == TransferDirection::Download && !transfer.file_data.is_empty() {
            if let Err(e) = std::fs::write(&transfer.local_path, &transfer.file_data) {
                warn!("FileTransfer: failed to write {}: {}", transfer.local_path.display(), e);
                transfer.error = Some(format!("Write failed: {}", e));
            } else {
                info!(
                    "FileTransfer: downloaded {} ({} bytes)",
                    transfer.local_path.display(),
                    transfer.file_data.len()
                );
            }
        }

        Some(TransferProgressEvent {
            id: done.id,
            direction: format!("{:?}", transfer.direction),
            filename: transfer.filename.clone(),
            bytes_transferred: transfer.bytes_transferred,
            total_bytes: transfer.total_bytes,
            percent: 100.0,
            completed: true,
            error: transfer.error.clone(),
        })
    }

    fn handle_error(&mut self, err: &FileTransferError) -> Option<TransferProgressEvent> {
        if let Some(transfer) = self.active.get_mut(&err.id) {
            transfer.completed = true;
            transfer.error = Some(err.error.clone());

            Some(TransferProgressEvent {
                id: err.id,
                direction: format!("{:?}", transfer.direction),
                filename: transfer.filename.clone(),
                bytes_transferred: transfer.bytes_transferred,
                total_bytes: transfer.total_bytes,
                percent: 0.0,
                completed: true,
                error: Some(err.error.clone()),
            })
        } else {
            warn!("FileTransfer: error for unknown id {}: {}", err.id, err.error);
            None
        }
    }
}

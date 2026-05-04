/// Yomie Agent — Remote File Browser
///
/// Provides directory listing and file metadata for the remote
/// file browser feature.  Used by the admin console to navigate
/// the device's file system.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A single entry in a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub size: u64,
    pub modified: Option<String>,
    pub readonly: bool,
    pub hidden: bool,
}

/// Result of a directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryListing {
    pub path: String,
    pub entries: Vec<FileEntry>,
    pub total: usize,
    pub parent: Option<String>,
}

/// Request to list a directory from the admin console.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseRequest {
    pub path: String,
    pub show_hidden: bool,
}

pub struct FileBrowser;

impl FileBrowser {
    /// List the contents of a directory.
    pub fn list_dir(path: &str, show_hidden: bool) -> Result<DirectoryListing> {
        let path_buf = PathBuf::from(path);

        if !path_buf.exists() {
            anyhow::bail!("Path does not exist: {}", path);
        }
        if !path_buf.is_dir() {
            anyhow::bail!("Not a directory: {}", path);
        }

        let mut entries = Vec::new();

        for entry in std::fs::read_dir(&path_buf)? {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let name = entry.file_name().to_string_lossy().into_owned();

            // Filter hidden files
            let hidden = is_hidden(&name, &entry.path());
            if !show_hidden && hidden {
                continue;
            }

            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| {
                    let dt: chrono::DateTime<chrono::Utc> = t.into();
                    Some(dt.to_rfc3339())
                });

            entries.push(FileEntry {
                name,
                path: entry.path().to_string_lossy().into_owned(),
                is_dir: metadata.is_dir(),
                is_file: metadata.is_file(),
                is_symlink: metadata.file_type().is_symlink(),
                size: if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                },
                modified,
                readonly: metadata.permissions().readonly(),
                hidden,
            });
        }

        // Sort: directories first, then alphabetically
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let total = entries.len();
        let parent = path_buf.parent().map(|p| p.to_string_lossy().into_owned());

        Ok(DirectoryListing {
            path: path_buf.to_string_lossy().into_owned(),
            entries,
            total,
            parent,
        })
    }

    /// List available drives / root paths.
    pub fn list_roots() -> Vec<FileEntry> {
        let mut roots = Vec::new();

        #[cfg(target_os = "windows")]
        {
            // List drive letters A-Z
            for letter in b'A'..=b'Z' {
                let drive = format!("{}:\\", letter as char);
                let path = PathBuf::from(&drive);
                if path.exists() {
                    roots.push(FileEntry {
                        name: drive.clone(),
                        path: drive,
                        is_dir: true,
                        is_file: false,
                        is_symlink: false,
                        size: 0,
                        modified: None,
                        readonly: false,
                        hidden: false,
                    });
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            roots.push(FileEntry {
                name: "/".to_string(),
                path: "/".to_string(),
                is_dir: true,
                is_file: false,
                is_symlink: false,
                size: 0,
                modified: None,
                readonly: false,
                hidden: false,
            });

            // Add home directory
            if let Some(home) = dirs::home_dir() {
                roots.push(FileEntry {
                    name: "Home".to_string(),
                    path: home.to_string_lossy().into_owned(),
                    is_dir: true,
                    is_file: false,
                    is_symlink: false,
                    size: 0,
                    modified: None,
                    readonly: false,
                    hidden: false,
                });
            }
        }

        roots
    }

    /// Get metadata for a single file.
    pub fn file_info(path: &str) -> Result<FileEntry> {
        let path_buf = PathBuf::from(path);
        let metadata = std::fs::metadata(&path_buf)?;

        let name = path_buf
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                Some(dt.to_rfc3339())
            });

        Ok(FileEntry {
            name,
            path: path_buf.to_string_lossy().into_owned(),
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            is_symlink: metadata.file_type().is_symlink(),
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
            modified,
            readonly: metadata.permissions().readonly(),
            hidden: is_hidden(
                path_buf
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(""),
                &path_buf,
            ),
        })
    }
}

/// Check if a file is hidden (platform-specific).
fn is_hidden(name: &str, _path: &Path) -> bool {
    // Unix: starts with dot
    if name.starts_with('.') {
        return true;
    }

    // Windows: check FILE_ATTRIBUTE_HIDDEN
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(metadata) = std::fs::metadata(_path) {
            const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
            return metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_roots() {
        let roots = FileBrowser::list_roots();
        assert!(!roots.is_empty());
        assert!(roots[0].is_dir);
    }

    #[test]
    fn test_list_temp_dir() {
        let temp = std::env::temp_dir();
        let listing =
            FileBrowser::list_dir(temp.to_str().unwrap(), false).unwrap();
        assert_eq!(
            listing.path,
            temp.to_string_lossy().into_owned()
        );
    }
}

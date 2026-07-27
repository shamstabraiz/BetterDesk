/**
 * codenextremote Web Remote Client - File Transfer Module
 * Handles RustDesk file transfer protocol: browse, download, upload, manage
 *
 * Protocol (names are from the remote agent's perspective — same as native RustDesk):
 *   Browse:   FileAction.read_dir → FileResponse.dir
 *   Download: FileAction.send → [digest → send_confirm] → FileResponse.block* → done
 *   Upload:   FileAction.receive → digest(is_upload) → send_confirm → FileResponse.block* → done
 *   Cancel:   FileAction.cancel
 */

/* global RDProtocol, fzstd */

// eslint-disable-next-line no-unused-vars
class RDFileTransfer {
    /**
     * @param {Object} opts
     * @param {RDProtocol} opts.proto - Protocol handler
     * @param {Function} opts.sendMessage - Function to send peer message: (msgObj) => void
     * @param {Function} opts.emit - Event emitter: (event, ...args) => void
     * @param {Function} [opts.diag] - Optional diagnostic reporter: (event, detail) => void
     */
    constructor(opts) {
        this._proto = opts.proto;
        this._sendMessage = opts.sendMessage;
        this._emit = opts.emit;
        this._diag = typeof opts.diag === 'function' ? opts.diag : null;

        /** @type {string} Current remote directory path */
        this._currentPath = '';

        /** @type {Array<Object>} Current directory entries */
        this._entries = [];

        /** @type {Map<number, Object>} Active transfers by ID */
        this._transfers = new Map();

        /** @type {number} Transfer ID counter */
        this._nextId = 1;

        /** @type {boolean} Whether file transfer is enabled */
        this._enabled = false;

        /** @type {boolean} Show hidden files */
        this._showHidden = false;

        /** @type {number|null} Timestamp when last browseDir was sent */
        this._browseSentAt = null;

        // File type constants from proto
        this.FILE_TYPE = {
            DIR: 0,
            DIR_LINK: 2,
            DIR_DRIVE: 3,
            FILE: 4,
            FILE_LINK: 5
        };

        // Block size for uploads (64KB, matching RustDesk default)
        this.BLOCK_SIZE = 65536;
    }

    /**
     * Join directory + name using remote path separator.
     * @param {string} dir
     * @param {string} name
     * @returns {string}
     */
    static joinPath(dir, name) {
        const n = name || '';
        if (!dir) return n;
        const sep = dir.includes('\\') ? '\\' : '/';
        if (dir.endsWith(sep)) return dir + n;
        return dir + sep + n;
    }

    /**
     * Enable file transfer (called after successful login)
     */
    enable() {
        this._enabled = true;
        console.log('[FileTransfer] enabled (client-side; waiting for peer FileResponse on browse)');
        this._emit('log', '[FileTransfer] client ready — open file panel to browse remote');
        this._reportDiag('ft_enabled', { detail: 'client_side_enabled' });
    }

    /**
     * Disable file transfer
     */
    disable() {
        this._enabled = false;
        this.cancelAll();
    }

    _reportDiag(event, extra) {
        if (!this._diag) return;
        try {
            this._diag(event, extra || {});
        } catch (err) {
            console.warn('[FileTransfer] diag report failed:', err.message);
        }
    }

    get enabled() { return this._enabled; }
    get currentPath() { return this._currentPath; }
    get entries() { return this._entries; }

    /**
     * Browse a directory on the remote machine
     * @param {string} [path=''] - Path to browse (empty = root/drives)
     */
    browseDir(path) {
        if (!this._enabled) {
            console.warn('[FileTransfer] browseDir called but file transfer not enabled');
            this._emit('log', '[FileTransfer] browse skipped — not enabled yet');
            this._reportDiag('browse_skipped_not_enabled', { path: path || '' });
            return;
        }
        const dir = path != null ? path : '';
        this._browseSentAt = Date.now();
        console.log('[FileTransfer] SEND FileAction.read_dir path=', JSON.stringify(dir),
            'hidden=', this._showHidden, 'at=', new Date(this._browseSentAt).toISOString());
        this._emit('log', '[FileTransfer] sent read_dir → waiting for FileResponse.dir (5s)');
        this._reportDiag('browse_sent', { path: dir, detail: 'FileAction.read_dir' });
        this._sendMessage(this._proto.buildReadDir(dir, this._showHidden));
        this._emit('file_browsing', { path: dir });

        // Set timeout — if no file_dir response within 5s, emit timeout event
        if (this._browseTimeout) clearTimeout(this._browseTimeout);
        this._browseTimedOut = false;
        this._browseTimeout = setTimeout(() => {
            if (!this._browseTimedOut) {
                this._browseTimedOut = true;
                const waited = this._browseSentAt ? (Date.now() - this._browseSentAt) : 5000;
                console.warn('[FileTransfer] TIMEOUT — no FileResponse.dir after', waited,
                    'ms. Likely peer ignored FileAction (old agent without in-session FT, or file permission off).');
                this._emit('log',
                    '[FileTransfer] TIMEOUT: peer did not answer read_dir — agent likely ignoring FileAction on desktop session');
                this._reportDiag('browse_timeout', {
                    path: dir,
                    detail: 'no_FileResponse_dir_after_' + waited + 'ms'
                });
                this._emit('file_browse_timeout', { path: dir, waitedMs: waited });
            }
        }, 5000);
    }

    /**
     * Navigate up to parent directory
     */
    browseParent() {
        if (!this._currentPath) return;
        // Handle both Windows and Unix paths
        let parent = this._currentPath.replace(/[\\/]+$/, '');
        const sep = parent.includes('\\') ? '\\' : '/';
        const lastSep = parent.lastIndexOf(sep);
        if (lastSep > 0) {
            parent = parent.substring(0, lastSep);
        } else if (lastSep === 0) {
            parent = sep; // Unix root
        } else {
            parent = ''; // Drive list on Windows
        }
        this.browseDir(parent);
    }

    /**
     * Toggle hidden file visibility
     * @param {boolean} show
     */
    setShowHidden(show) {
        this._showHidden = !!show;
        // Refresh current directory
        if (this._enabled && this._currentPath !== undefined) {
            this.browseDir(this._currentPath);
        }
    }

    /**
     * Download a file from remote (FileAction.send — ask peer to send the file).
     * @param {string} remotePath - Remote directory path
     * @param {Object} fileEntry - FileEntry { name, size, modified_time, entry_type }
     * @returns {number} Transfer ID
     */
    downloadFile(remotePath, fileEntry) {
        if (!this._enabled) return -1;

        const id = this._nextId++;
        const fullPath = RDFileTransfer.joinPath(remotePath || '', fileEntry.name);
        const transfer = {
            id: id,
            type: 'download',
            remotePath: remotePath,
            fullPath: fullPath,
            fileName: fileEntry.name,
            fileSize: Number(fileEntry.size || 0),
            receivedBytes: 0,
            blocks: [],
            startTime: Date.now(),
            status: 'pending', // pending → transferring → complete → error
            fileNum: 0,
            blockCount: 0
        };
        this._transfers.set(id, transfer);

        // Native: new_send — peer reads and streams to us
        console.log('[FileTransfer] SEND FileAction.send (download) id=', id,
            'path=', JSON.stringify(fullPath));
        this._reportDiag('download_send', { path: fullPath, detail: 'id=' + id });
        this._sendMessage(this._proto.buildFileSendRequest(
            id, fullPath, this._showHidden, 0
        ));

        this._emit('file_transfer_start', {
            id: id,
            type: 'download',
            fileName: fileEntry.name,
            fileSize: transfer.fileSize
        });

        return id;
    }

    /**
     * Upload a file to remote (FileAction.receive — ask peer to receive/write the file).
     * @param {File} file - Browser File object
     * @param {string} remotePath - Remote destination directory
     * @returns {number} Transfer ID
     */
    uploadFile(file, remotePath) {
        if (!this._enabled) return -1;

        const id = this._nextId++;
        const destPath = RDFileTransfer.joinPath(remotePath || '', file.name);
        const modifiedTime = Math.floor((file.lastModified || Date.now()) / 1000);
        const transfer = {
            id: id,
            type: 'upload',
            remotePath: remotePath,
            fullPath: destPath,
            fileName: file.name,
            fileSize: file.size,
            sentBytes: 0,
            file: file,
            startTime: Date.now(),
            status: 'pending',
            fileNum: 0,
            currentBlk: 0
        };
        this._transfers.set(id, transfer);

        // Native single-file upload: path = full remote file path, files[0].name = ""
        // (agent does join(path, name); a non-empty name would create path/as/dir/name)
        const files = [{
            entryType: this.FILE_TYPE.FILE,
            name: '',
            size: file.size,
            modifiedTime: modifiedTime,
            isHidden: false
        }];
        console.log('[FileTransfer] SEND FileAction.receive (upload) id=', id,
            'path=', JSON.stringify(destPath), 'size=', file.size);
        this._reportDiag('upload_receive', {
            path: destPath,
            detail: 'id=' + id + ' size=' + file.size
        });
        this._sendMessage(this._proto.buildFileReceiveRequest(
            id, destPath, files, 0, file.size
        ));

        // Native read-job with overwrite detection: send digest, wait for send_confirm, then blocks.
        // Also start a short fallback so OD-off peers still receive data.
        this._sendMessage(this._proto.buildFileDigest(
            id, 0, modifiedTime, file.size, false
        ));
        console.log('[FileTransfer] SEND FileResponse.digest (upload meta) id=', id);
        if (transfer._uploadStartTimer) clearTimeout(transfer._uploadStartTimer);
        transfer._uploadStartTimer = setTimeout(() => {
            if (transfer.status === 'pending' && this._transfers.has(id)) {
                console.log('[FileTransfer] upload start fallback (no confirm yet) id=', id);
                this._sendUploadBlocks(transfer);
            }
        }, 800);

        this._emit('file_transfer_start', {
            id: id,
            type: 'upload',
            fileName: file.name,
            fileSize: file.size
        });

        return id;
    }

    /**
     * Cancel a transfer
     * @param {number} id
     */
    cancelTransfer(id) {
        const transfer = this._transfers.get(id);
        if (!transfer) return;

        transfer.status = 'cancelled';
        this._sendMessage(this._proto.buildFileCancel(id));
        this._transfers.delete(id);

        this._emit('file_transfer_cancelled', { id: id, fileName: transfer.fileName });
    }

    /**
     * Cancel all active transfers
     */
    cancelAll() {
        for (const [id] of this._transfers) {
            this.cancelTransfer(id);
        }
    }

    /**
     * Create directory on remote
     * @param {string} path
     */
    createDir(path) {
        if (!this._enabled) return;
        const id = this._nextId++;
        this._sendMessage(this._proto.buildFileDirCreate(id, path));
        this._emit('file_action', { action: 'create_dir', path: path });
    }

    /**
     * Delete file on remote
     * @param {string} path
     */
    removeFile(path) {
        if (!this._enabled) return;
        const id = this._nextId++;
        this._sendMessage(this._proto.buildFileRemove(id, path, 0));
        this._emit('file_action', { action: 'remove_file', path: path });
    }

    /**
     * Delete directory on remote
     * @param {string} path
     * @param {boolean} recursive
     */
    removeDir(path, recursive) {
        if (!this._enabled) return;
        const id = this._nextId++;
        this._sendMessage(this._proto.buildFileRemoveDir(id, path, recursive));
        this._emit('file_action', { action: 'remove_dir', path: path });
    }

    /**
     * Rename file/directory on remote
     * @param {string} path
     * @param {string} newName
     */
    rename(path, newName) {
        if (!this._enabled) return;
        const id = this._nextId++;
        this._sendMessage(this._proto.buildFileRename(id, path, newName));
        this._emit('file_action', { action: 'rename', path: path, newName: newName });
    }

    // ---- Incoming message handlers ----

    /**
     * Handle FileResponse from peer
     * @param {Object} resp - Decoded FileResponse protobuf
     */
    handleFileResponse(resp) {
        const kinds = Object.keys(resp).filter(k => resp[k] != null);
        console.log('[FileTransfer] RECV FileResponse:', kinds.join(', '));
        this._reportDiag('file_response', { detail: kinds.join(',') || 'empty' });
        if (resp.dir) {
            this._handleDir(resp.dir);
        } else if (resp.block) {
            this._handleBlock(resp.block);
        } else if (resp.digest) {
            this._handleDigest(resp.digest);
        } else if (resp.done) {
            this._handleDone(resp.done);
        } else if (resp.error) {
            this._handleError(resp.error);
        }
    }

    /**
     * Handle FileAction from peer (e.g. send_confirm after upload digest).
     * @param {Object} action
     */
    handleFileAction(action) {
        if (!action) return;
        if (action.sendConfirm || action.send_confirm) {
            this._handleSendConfirm(action.sendConfirm || action.send_confirm);
        }
    }

    /**
     * Peer confirmed our upload digest — start streaming blocks.
     * @param {Object} confirm
     */
    _handleSendConfirm(confirm) {
        const id = confirm.id;
        const transfer = this._transfers.get(id);
        console.log('[FileTransfer] RECV send_confirm id=', id,
            'skip=', !!(confirm.skip), 'offsetBlk=', confirm.offsetBlk != null ? confirm.offsetBlk : confirm.offset_blk);
        if (!transfer) return;
        if (confirm.skip) {
            transfer.status = 'complete';
            this._transfers.delete(id);
            this._emit('file_transfer_complete', {
                id: id,
                fileName: transfer.fileName,
                fileSize: transfer.fileSize,
                type: transfer.type,
                elapsed: (Date.now() - transfer.startTime) / 1000
            });
            return;
        }
        if (transfer.type === 'upload' && transfer.status === 'pending') {
            this._sendUploadBlocks(transfer);
        }
    }

    /**
     * Handle directory listing response
     * @param {Object} dir - FileDirectory { id, path, entries[] }
     */
    _handleDir(dir) {
        // Clear browse timeout — we got a response
        if (this._browseTimeout) {
            clearTimeout(this._browseTimeout);
            this._browseTimeout = null;
        }
        this._browseTimedOut = false;

        const elapsed = this._browseSentAt ? (Date.now() - this._browseSentAt) : null;
        console.log('[FileTransfer] RECV dir path=%s entries=%d elapsedMs=%s',
            dir.path || '(root)', (dir.entries || []).length, elapsed != null ? elapsed : '?');
        this._emit('log',
            '[FileTransfer] peer answered read_dir (' + (dir.entries || []).length + ' entries' +
            (elapsed != null ? ', ' + elapsed + 'ms' : '') + ')');
        this._reportDiag('browse_ok', {
            path: dir.path || '',
            detail: 'entries=' + (dir.entries || []).length + (elapsed != null ? ' elapsedMs=' + elapsed : '')
        });
        this._currentPath = dir.path || '';
        this._entries = (dir.entries || []).map(e => ({
            name: e.name,
            entryType: e.entryType != null ? e.entryType : (e.entry_type != null ? e.entry_type : 0),
            isHidden: !!e.isHidden,
            size: Number(e.size || 0),
            modifiedTime: Number(e.modifiedTime || e.modified_time || 0),
            isDir: (e.entryType || e.entry_type || 0) <= 3
        }));

        // Sort: directories first, then by name
        this._entries.sort((a, b) => {
            if (a.isDir && !b.isDir) return -1;
            if (!a.isDir && b.isDir) return 1;
            return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
        });

        this._emit('file_dir', {
            path: this._currentPath,
            entries: this._entries
        });
    }

    /**
     * Handle transfer digest (file metadata before data blocks)
     * @param {Object} digest - FileTransferDigest
     */
    _handleDigest(digest) {
        const transfer = this._transfers.get(digest.id);
        const isUpload = !!(digest.isUpload != null ? digest.isUpload : digest.is_upload);
        const fileSize = Number(digest.fileSize != null ? digest.fileSize : (digest.file_size || 0));
        const fileNum = digest.fileNum != null ? digest.fileNum : (digest.file_num || 0);

        console.log('[FileTransfer] RECV digest id=', digest.id,
            'fileNum=', fileNum, 'fileSize=', fileSize, 'isUpload=', isUpload);

        if (!transfer) {
            console.warn('[FileTransfer] digest for unknown transfer id=', digest.id);
            this._reportDiag('digest_unknown', { detail: 'id=' + digest.id });
            return;
        }

        if (fileSize > 0) transfer.fileSize = fileSize;
        if (fileNum != null) transfer.fileNum = fileNum;
        transfer.status = 'transferring';

        // Always confirm (native OffsetBlk) — required for is_upload and overwrite detection
        this._sendMessage(this._proto.buildFileSendConfirm(
            digest.id, fileNum, false, 0
        ));
        this._reportDiag('digest_confirm', {
            path: transfer.fullPath || transfer.fileName,
            detail: 'id=' + digest.id + ' isUpload=' + isUpload + ' size=' + transfer.fileSize
        });

        if (transfer.type === 'upload' || isUpload) {
            if (transfer._uploadStartTimer) {
                clearTimeout(transfer._uploadStartTimer);
                transfer._uploadStartTimer = null;
            }
            // Peer asked for confirm on conflict, or echoed digest — confirm then stream
            if (transfer.status === 'transferring' || transfer.status === 'pending') {
                this._sendUploadBlocks(transfer);
            }
        }

        this._emit('file_transfer_progress', {
            id: digest.id,
            fileName: transfer.fileName,
            fileSize: transfer.fileSize,
            transferred: transfer.type === 'upload' ? (transfer.sentBytes || 0) : (transfer.receivedBytes || 0),
            percent: 0,
            type: transfer.type
        });
    }

    /**
     * Decompress a zstd FileTransferBlock payload (native RustDesk compress()).
     * @param {Uint8Array} data
     * @returns {Uint8Array}
     */
    _decompressBlock(data) {
        const bytes = data instanceof Uint8Array ? data : new Uint8Array(data);
        if (typeof fzstd === 'undefined' || typeof fzstd.decompress !== 'function') {
            throw new Error('zstd decompressor (fzstd) not loaded');
        }
        const out = fzstd.decompress(bytes);
        return out instanceof Uint8Array ? out : new Uint8Array(out);
    }

    /**
     * Handle data block (download)
     * @param {Object} block - FileTransferBlock { id, file_num, data, compressed, blk_id }
     */
    _handleBlock(block) {
        const transfer = this._transfers.get(block.id);
        if (!transfer || transfer.type !== 'download') return;

        transfer.status = 'transferring';
        let payload = block.data;
        if (!payload || !payload.length) return;

        const compressed = !!(block.compressed != null ? block.compressed : block.Compressed);
        try {
            if (compressed) {
                payload = this._decompressBlock(payload);
            } else if (!(payload instanceof Uint8Array)) {
                payload = new Uint8Array(payload);
            }
        } catch (err) {
            console.warn('[FileTransfer] block decompress failed:', err.message);
            transfer.status = 'error';
            this._transfers.delete(block.id);
            this._emit('file_transfer_error', {
                id: block.id,
                fileName: transfer.fileName,
                error: 'Decompress failed: ' + (err.message || 'unknown')
            });
            return;
        }

        transfer.blocks.push(payload);
        transfer.receivedBytes += payload.length;
        transfer.blockCount = (transfer.blockCount || 0) + 1;

        if (transfer.blockCount === 1) {
            console.log('[FileTransfer] first download block id=', block.id,
                'compressed=', compressed, 'raw=', block.data.length,
                'out=', payload.length);
            this._reportDiag('download_first_block', {
                detail: 'compressed=' + compressed + ' out=' + payload.length
            });
        }

        const percent = transfer.fileSize > 0
            ? Math.min(100, Math.round((transfer.receivedBytes / transfer.fileSize) * 100))
            : 0;

        this._emit('file_transfer_progress', {
            id: block.id,
            fileName: transfer.fileName,
            fileSize: transfer.fileSize,
            transferred: transfer.receivedBytes,
            percent: percent,
            type: 'download'
        });
    }

    /**
     * Handle transfer done
     * @param {Object} done - FileTransferDone { id, file_num }
     */
    _handleDone(done) {
        const transfer = this._transfers.get(done.id);
        if (!transfer) return;

        transfer.status = 'complete';
        const elapsed = (Date.now() - transfer.startTime) / 1000;

        if (transfer.type === 'download') {
            // Assemble and trigger browser download
            this._triggerDownload(transfer);
        }

        this._emit('file_transfer_complete', {
            id: done.id,
            fileName: transfer.fileName,
            fileSize: transfer.fileSize,
            type: transfer.type,
            elapsed: elapsed
        });

        this._transfers.delete(done.id);

        // Refresh directory listing after upload
        if (transfer.type === 'upload') {
            this.browseDir(this._currentPath);
        }
    }

    /**
     * Handle transfer error
     * @param {Object} error - FileTransferError { id, error, file_num }
     */
    _handleError(error) {
        const transfer = this._transfers.get(error.id);
        const fileName = transfer ? transfer.fileName : 'unknown';

        if (transfer) {
            transfer.status = 'error';
            this._transfers.delete(error.id);
        }

        this._emit('file_transfer_error', {
            id: error.id,
            fileName: fileName,
            error: error.error || 'Unknown error'
        });
    }

    // ---- Upload block streaming ----

    /**
     * Stream file blocks for upload
     * @param {Object} transfer
     */
    async _sendUploadBlocks(transfer) {
        const file = transfer.file;
        if (!file) return;
        if (transfer._uploading) return;
        transfer._uploading = true;
        if (transfer._uploadStartTimer) {
            clearTimeout(transfer._uploadStartTimer);
            transfer._uploadStartTimer = null;
        }

        try {
            transfer.status = 'transferring';
            let offset = 0;
            let blkId = 0;

            while (offset < file.size && transfer.status === 'transferring') {
                const end = Math.min(offset + this.BLOCK_SIZE, file.size);
                const slice = file.slice(offset, end);
                const data = new Uint8Array(await slice.arrayBuffer());

                this._sendMessage(this._proto.buildFileBlock(
                    transfer.id, transfer.fileNum, data, false, blkId
                ));

                transfer.sentBytes = end;
                blkId++;
                offset = end;

                if (blkId === 1) {
                    console.log('[FileTransfer] first upload block id=', transfer.id,
                        'bytes=', data.length, 'fileSize=', file.size);
                    this._reportDiag('upload_first_block', {
                        detail: 'id=' + transfer.id + ' bytes=' + data.length
                    });
                }

                const percent = Math.min(100, Math.round((end / file.size) * 100));
                this._emit('file_transfer_progress', {
                    id: transfer.id,
                    fileName: transfer.fileName,
                    fileSize: transfer.fileSize,
                    transferred: end,
                    percent: percent,
                    type: 'upload'
                });

                // Yield to event loop every 16 blocks to avoid blocking UI
                if (blkId % 16 === 0) {
                    await new Promise(r => setTimeout(r, 0));
                }
            }

            // Send done
            if (transfer.status === 'transferring') {
                console.log('[FileTransfer] upload done id=', transfer.id,
                    'bytes=', transfer.sentBytes, 'blocks=', blkId);
                this._reportDiag('upload_done', {
                    path: transfer.fullPath || transfer.fileName,
                    detail: 'bytes=' + transfer.sentBytes
                });
                this._sendMessage(this._proto.buildFileDone(transfer.id, transfer.fileNum));
            }
        } catch (err) {
            transfer.status = 'error';
            this._emit('file_transfer_error', {
                id: transfer.id,
                fileName: transfer.fileName,
                error: err.message || 'Upload failed'
            });
            this._transfers.delete(transfer.id);
        } finally {
            transfer._uploading = false;
        }
    }

    // ---- Browser download trigger ----

    /**
     * Assemble received blocks into a Blob and trigger download
     * @param {Object} transfer
     */
    _triggerDownload(transfer) {
        if (!transfer.blocks.length) return;

        try {
            const blob = new Blob(transfer.blocks, { type: 'application/octet-stream' });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url;
            a.download = transfer.fileName;
            a.style.display = 'none';
            document.body.appendChild(a);
            a.click();
            // Cleanup
            setTimeout(() => {
                URL.revokeObjectURL(url);
                a.remove();
            }, 5000);
        } catch (err) {
            this._emit('file_transfer_error', {
                id: transfer.id,
                fileName: transfer.fileName,
                error: 'Failed to save file: ' + (err.message || 'unknown error')
            });
        }
    }

    // ---- Utility ----

    /**
     * Format file size for display
     * @param {number} bytes
     * @returns {string}
     */
    static formatSize(bytes) {
        if (bytes === 0) return '0 B';
        const units = ['B', 'KB', 'MB', 'GB', 'TB'];
        const i = Math.floor(Math.log(bytes) / Math.log(1024));
        return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + ' ' + units[i];
    }

    /**
     * Format timestamp to locale string
     * @param {number} ts - Unix timestamp in seconds
     * @returns {string}
     */
    static formatTime(ts) {
        if (!ts) return '';
        return new Date(ts * 1000).toLocaleString();
    }

    /**
     * Get icon name for file entry type
     * @param {Object} entry
     * @returns {string} Material Icons name
     */
    static getFileIcon(entry) {
        if (entry.isDir) {
            if (entry.entryType === 3) return 'storage'; // Drive
            return 'folder';
        }
        const ext = (entry.name || '').split('.').pop().toLowerCase();
        const imageExts = ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp', 'svg', 'ico'];
        const videoExts = ['mp4', 'avi', 'mkv', 'mov', 'wmv', 'flv', 'webm'];
        const audioExts = ['mp3', 'wav', 'ogg', 'flac', 'aac', 'wma', 'm4a'];
        const docExts = ['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'rtf', 'csv'];
        const codeExts = ['js', 'ts', 'py', 'go', 'rs', 'java', 'c', 'cpp', 'h', 'css', 'html', 'json', 'xml', 'yml', 'yaml', 'toml', 'sh', 'bat', 'ps1'];
        const archiveExts = ['zip', 'tar', 'gz', 'rar', '7z', 'bz2', 'xz'];

        if (imageExts.includes(ext)) return 'image';
        if (videoExts.includes(ext)) return 'movie';
        if (audioExts.includes(ext)) return 'music_note';
        if (docExts.includes(ext)) return 'description';
        if (codeExts.includes(ext)) return 'code';
        if (archiveExts.includes(ext)) return 'archive';
        if (ext === 'exe' || ext === 'msi') return 'apps';
        return 'insert_drive_file';
    }

    /**
     * Get transfer statistics
     * @returns {Object}
     */
    getStats() {
        const active = [];
        for (const [, t] of this._transfers) {
            const transferred = t.type === 'download' ? t.receivedBytes : (t.sentBytes || 0);
            const elapsed = (Date.now() - t.startTime) / 1000;
            active.push({
                id: t.id,
                type: t.type,
                fileName: t.fileName,
                fileSize: t.fileSize,
                transferred: transferred,
                percent: t.fileSize > 0 ? Math.round((transferred / t.fileSize) * 100) : 0,
                speed: elapsed > 0 ? transferred / elapsed : 0,
                status: t.status
            });
        }
        return { active: active, count: active.length };
    }
}

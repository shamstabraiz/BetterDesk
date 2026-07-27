/**
 * codenextremote Web Remote Client — Dedicated FILE_TRANSFER session
 *
 * Native RustDesk opens a second connection with ConnType.FILE_TRANSFER for
 * file browse/transfer. Old agents ignore FileAction on a desktop (DEFAULT_CONN)
 * session. This module opens that dedicated session alongside the video session
 * so the in-panel file browser works without requiring a patched agent.
 *
 * Flow mirrors RDClient.connect/authenticate, but:
 *   - PunchHole / RequestRelay use ConnType.FILE_TRANSFER
 *   - LoginRequest carries file_transfer { dir, show_hidden }
 *   - Only FileResponse / login / crypto frames are handled
 */

/* global RDConnection, RDCrypto, RDProtocol, nacl */

// eslint-disable-next-line no-unused-vars
class RDFileSession {
    /**
     * @param {Object} opts
     * @param {string} opts.deviceId
     * @param {RDProtocol} opts.proto - Shared loaded protocol instance
     * @param {string} [opts.serverPubKey]
     * @param {string} [opts.myName]
     * @param {string} [opts.myId] - Must match desktop LoginRequest.my_id for session reuse
     * @param {number} [opts.sessionId] - Must match desktop LoginRequest.session_id
     * @param {string} [opts.cached2faCode] - Last desktop 2FA code to try automatically
     * @param {Function} opts.emit - (event, ...args) => void
     * @param {Function} [opts.diag] - (event, extra) => void
     * @param {Function} [opts.onFileResponse] - (fileResponse) => void
     * @param {Function} [opts.onFileAction] - (fileAction) => void
     * @param {Function} [opts.on2faRequired] - () => Promise<string> resolve with TOTP code
     */
    constructor(opts) {
        this.deviceId = opts.deviceId;
        this.proto = opts.proto;
        this.serverPubKey = opts.serverPubKey || '';
        this.myName = opts.myName || 'codenextremote Web';
        this.myId = opts.myId || ('codenextremote-web-' + Date.now().toString(36));
        this.sessionId = opts.sessionId != null ? opts.sessionId : Date.now();
        this._cached2faCode = opts.cached2faCode || null;
        this._emit = opts.emit || (() => {});
        this._diag = typeof opts.diag === 'function' ? opts.diag : null;
        this._onFileResponse = typeof opts.onFileResponse === 'function'
            ? opts.onFileResponse : null;
        this._onFileAction = typeof opts.onFileAction === 'function'
            ? opts.onFileAction : null;
        this._on2faRequired = typeof opts.on2faRequired === 'function'
            ? opts.on2faRequired : null;

        this.conn = new RDConnection();
        this.crypto = new RDCrypto();

        this._state = 'idle'; // idle | connecting | authenticating | waiting_2fa | ready | error | closed
        this._relayDecoder = null;
        this._relayConfirmReceived = false;
        this._relayFrameIdx = 0;
        this._keyExchangePending = false;
        this._keyExchangeDone = false;
        this._peerEncryptionConfirmed = false;
        this._peerSignedPk = null;
        this._loginChallenge = null;
        this._loginSalt = null;
        this._password = null;
        this._connectPromise = null;
        this._readyResolve = null;
        this._readyReject = null;
        this._readyTimer = null;
        /** @type {boolean} Peer auto-sent FileResponse.dir after FT login */
        this._initialDirReceived = false;
        this._2faTriedCached = false;
    }

    get state() { return this._state; }
    get ready() { return this._state === 'ready'; }
    get initialDirReceived() { return this._initialDirReceived; }

    _report(event, extra) {
        if (!this._diag) return;
        try { this._diag(event, extra || {}); } catch (_) { /* ignore */ }
    }

    /**
     * Open (or reuse) a FILE_TRANSFER session and authenticate with password.
     * @param {string} password - Plaintext session password (same as desktop session)
     * @returns {Promise<void>}
     */
    ensure(password) {
        if (this._state === 'ready') {
            return Promise.resolve();
        }
        if (this._connectPromise) {
            return this._connectPromise;
        }
        if (!password) {
            return Promise.reject(new Error('file_session_password_required'));
        }
        this._password = password;
        this._connectPromise = this._connectAndAuth()
            .finally(() => {
                this._connectPromise = null;
            });
        return this._connectPromise;
    }

    async _connectAndAuth() {
        this._state = 'connecting';
        this._initialDirReceived = false;
        this._2faTriedCached = false;
        this._emit('log', '[FileSession] opening dedicated FILE_TRANSFER connection...');
        this._report('ft_session_connect', { detail: 'ConnType.FILE_TRANSFER' });

        this._relayDecoder = this.proto.createStreamDecoder();
        this._relayConfirmReceived = false;
        this._relayFrameIdx = 0;
        this._keyExchangePending = false;
        this._keyExchangeDone = false;
        this._peerEncryptionConfirmed = false;
        this.crypto = new RDCrypto();

        await this.conn.connectRendezvous();

        const ftConn = this.proto.enums.ConnType.values.FILE_TRANSFER;
        const punchHole = this.proto.buildPunchHoleRequest(
            this.deviceId, this.serverPubKey, ftConn
        );
        this.conn.sendRendezvous(this.proto.encodeRendezvous(punchHole));

        const rendezvousResponse = await this._waitForRendezvousResponse();
        if (rendezvousResponse.error) {
            throw new Error('File session refused: ' + rendezvousResponse.error);
        }
        this._peerSignedPk = rendezvousResponse.pk || null;

        let relayUUID = rendezvousResponse.uuid || '';
        let relayServer = rendezvousResponse.relayServer || '';

        if (!relayUUID) {
            relayUUID = (window.crypto && window.crypto.randomUUID
                ? window.crypto.randomUUID()
                : 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, c => {
                    const r = Math.random() * 16 | 0;
                    return (c === 'x' ? r : (r & 0x3 | 0x8)).toString(16);
                }));

            const requestRelaySignal = this.proto.buildRequestRelay(
                this.deviceId, relayUUID, relayServer, this.serverPubKey, ftConn
            );
            this.conn.sendRendezvous(this.proto.encodeRendezvous(requestRelaySignal));

            const relayConfirm = await this._waitForSignalRelayResponse();
            if (relayConfirm.error) {
                throw new Error('File relay refused: ' + relayConfirm.error);
            }
            relayUUID = relayConfirm.uuid || relayUUID;
            relayServer = relayConfirm.relayServer || relayServer;
            if (relayConfirm.pk) this._peerSignedPk = relayConfirm.pk;
        }

        this.conn.closeRendezvous();
        await this.conn.connectRelay();

        this.conn.on('relay:message', (data) => this._handleRelayData(data));
        this.conn.on('relay:close', () => this._onClosed('relay closed'));
        this.conn.on('relay:error', (e) => this._onClosed('relay error: ' + (e && e.message)));

        const requestRelay = this.proto.buildRequestRelay(
            this.deviceId, relayUUID, relayServer, this.serverPubKey, ftConn
        );
        this.conn.sendRelay(this.proto.encodeRendezvous(requestRelay));

        // Wait until LoginResponse succeeds (or error / timeout)
        await new Promise((resolve, reject) => {
            this._readyResolve = resolve;
            this._readyReject = reject;
            this._readyTimer = setTimeout(() => {
                reject(new Error('file_session_timeout'));
            }, 30000);
        });
    }

    _finishReady() {
        if (this._readyTimer) {
            clearTimeout(this._readyTimer);
            this._readyTimer = null;
        }
        this._state = 'ready';
        this._emit('log', '[FileSession] ready — FileAction will use FILE_TRANSFER connection');
        this._report('ft_session_ready', { detail: 'FILE_TRANSFER authenticated' });
        if (this._readyResolve) {
            const r = this._readyResolve;
            this._readyResolve = null;
            this._readyReject = null;
            r();
        }
    }

    _fail(err) {
        if (this._readyTimer) {
            clearTimeout(this._readyTimer);
            this._readyTimer = null;
        }
        this._state = 'error';
        const e = err instanceof Error ? err : new Error(String(err));
        this._emit('log', '[FileSession] failed: ' + e.message);
        this._report('ft_session_error', { detail: e.message });
        if (this._readyReject) {
            const r = this._readyReject;
            this._readyResolve = null;
            this._readyReject = null;
            r(e);
        }
    }

    _onClosed(reason) {
        if (this._state === 'closed' || this._state === 'idle') return;
        const wasReady = this._state === 'ready';
        this._state = 'closed';
        this._emit('log', '[FileSession] closed: ' + reason);
        this._report('ft_session_closed', { detail: String(reason || '') });
        if (!wasReady && this._readyReject) {
            this._fail(new Error(reason || 'file_session_closed'));
        }
    }

    /**
     * Send a peer Message over the FILE_TRANSFER relay (encrypted if enabled).
     * @param {Object} msgObj
     */
    send(msgObj) {
        if (!this.proto.loaded) return;
        if (this._state !== 'ready' && this._state !== 'authenticating' &&
            this._state !== 'connecting' && this._state !== 'waiting_2fa') {
            console.warn('[FileSession] send ignored — state=', this._state);
            return;
        }
        let data = this.proto.serializeMessage(msgObj);
        if (this.crypto.enabled) {
            data = this.crypto.processOutgoing(data);
        }
        this.conn.sendRelay(this.proto.frameBytes(data));
    }

    close() {
        if (this._readyTimer) {
            clearTimeout(this._readyTimer);
            this._readyTimer = null;
        }
        this._readyResolve = null;
        this._readyReject = null;
        this._state = 'closed';
        try { this.conn.close(); } catch (_) { /* ignore */ }
    }

    // ---- Rendezvous waits (same pattern as RDClient) ----

    _waitForRendezvousResponse() {
        return new Promise((resolve, reject) => {
            const decoder = this.proto.createStreamDecoder();
            const timeout = setTimeout(() => {
                this.conn.off('rendezvous:message', handler);
                reject(new Error('File session rendezvous timeout'));
            }, 30000);

            const handler = (rawData) => {
                const frames = decoder.feed(rawData);
                for (const frame of frames) {
                    try {
                        const msg = this.proto.decodeRendezvous(frame);
                        if (msg.keyExchange || msg.hc) continue;

                        if (msg.punchHoleResponse) {
                            clearTimeout(timeout);
                            this.conn.off('rendezvous:message', handler);
                            const resp = msg.punchHoleResponse;
                            const hasRelay = resp.relayServer && resp.relayServer.length > 0;
                            const hasSocket = resp.socketAddr && resp.socketAddr.length > 0;
                            if (hasRelay || hasSocket) {
                                resolve({
                                    relayServer: resp.relayServer || '',
                                    uuid: resp.uuid || '',
                                    pk: resp.pk || null
                                });
                            } else {
                                const failureNames = {
                                    0: 'Device not found',
                                    2: 'Device offline',
                                    3: 'License mismatch',
                                    4: 'Too many connections'
                                };
                                resolve({
                                    error: resp.otherFailure
                                        || failureNames[resp.failure]
                                        || ('Unknown error (code: ' + resp.failure + ')')
                                });
                            }
                            return;
                        }

                        if (msg.relayResponse) {
                            clearTimeout(timeout);
                            this.conn.off('rendezvous:message', handler);
                            const rr = msg.relayResponse;
                            if (rr.refuseReason && rr.refuseReason.length > 0) {
                                resolve({ error: rr.refuseReason });
                            } else {
                                resolve({
                                    uuid: rr.uuid || '',
                                    relayServer: rr.relayServer || '',
                                    pk: rr.pk || null
                                });
                            }
                            return;
                        }
                    } catch (err) {
                        console.warn('[FileSession] rendezvous decode:', err.message);
                    }
                }
            };
            this.conn.on('rendezvous:message', handler);
        });
    }

    _waitForSignalRelayResponse() {
        return new Promise((resolve, reject) => {
            const decoder = this.proto.createStreamDecoder();
            const timeout = setTimeout(() => {
                this.conn.off('rendezvous:message', handler);
                reject(new Error('File session relay signal timeout'));
            }, 30000);

            const handler = (rawData) => {
                const frames = decoder.feed(rawData);
                for (const frame of frames) {
                    try {
                        const msg = this.proto.decodeRendezvous(frame);
                        if (msg.keyExchange || msg.hc) continue;
                        if (msg.relayResponse) {
                            clearTimeout(timeout);
                            this.conn.off('rendezvous:message', handler);
                            const rr = msg.relayResponse;
                            if (rr.refuseReason && rr.refuseReason.length > 0) {
                                resolve({ error: rr.refuseReason });
                            } else {
                                resolve({
                                    uuid: rr.uuid || '',
                                    relayServer: rr.relayServer || '',
                                    pk: rr.pk || null
                                });
                            }
                            return;
                        }
                    } catch (err) {
                        console.warn('[FileSession] relay signal decode:', err.message);
                    }
                }
            };
            this.conn.on('rendezvous:message', handler);
        });
    }

    // ---- Relay message handling ----

    _handleRelayData(rawData) {
        try {
            const frames = this._relayDecoder.feed(rawData);
            for (const frame of frames) {
                this._handleRelayMessage(frame);
            }
        } catch (err) {
            console.warn('[FileSession] relay decode error:', err.message);
        }
    }

    _handleRelayMessage(frameData) {
        this._relayFrameIdx++;

        if (!this._relayConfirmReceived) {
            this._relayConfirmReceived = true;
            try {
                const rdvMsg = this.proto.decodeRendezvous(frameData);
                if (rdvMsg.relayResponse) {
                    console.log('[FileSession] relay confirmation skipped');
                    return;
                }
            } catch (_e) { /* not confirmation */ }
        }

        let data = frameData;

        if (this._keyExchangePending && !this._keyExchangeDone) {
            let isPlaintext = false;
            try {
                const probe = this.proto.decodeMessage(frameData);
                const fields = Object.keys(probe).filter(k => probe[k] != null && k !== 'union');
                if (fields.length > 0) isPlaintext = true;
            } catch (_e) { /* encrypted or garbage */ }

            if (isPlaintext) {
                // Peer is not encrypting — abandon key exchange (same as RDClient).
                this._keyExchangePending = false;
                this._keyExchangeDone = false;
                console.log('[FileSession] peer plaintext — skip key exchange');
            } else {
                this._sendPublicKey();
                this.crypto.enabled = true;
                this.crypto._sendSeq = 0;
                this.crypto._recvSeq = 0;
                this._keyExchangePending = false;
                this._keyExchangeDone = true;
            }
        }

        if (this.crypto.secretKey && this._keyExchangeDone) {
            const spec = this.crypto.tryDecrypt(new Uint8Array(data));
            if (spec) {
                this.crypto.commitDecrypt(spec.seq);
                data = spec.plaintext;
                this._peerEncryptionConfirmed = true;
            } else if (this._peerEncryptionConfirmed) {
                console.warn('[FileSession] decrypt failed after peer was encrypting');
                return;
            }
        }

        let msg;
        try {
            msg = this.proto.decodeMessage(data);
        } catch (err) {
            console.warn('[FileSession] message decode failed:', err.message);
            return;
        }

        if (msg.signedId) {
            this._handleSignedId(msg.signedId);
            return;
        }
        if (msg.hash) {
            this._handleHash(msg.hash);
            return;
        }
        if (msg.loginResponse) {
            this._handleLoginResponse(msg.loginResponse);
            return;
        }
        if (msg.fileResponse) {
            if (msg.fileResponse.dir) {
                this._initialDirReceived = true;
            }
            if (this._onFileResponse) this._onFileResponse(msg.fileResponse);
            // Auto-dir after login can arrive before LoginResponse is fully processed;
            // if we are still waiting on ready, finish now that auth clearly succeeded.
            if (this._state === 'authenticating' || this._state === 'waiting_2fa') {
                this._finishReady();
            }
            return;
        }
        if (msg.fileAction) {
            if (this._onFileAction) this._onFileAction(msg.fileAction);
            return;
        }
        if (msg.misc && msg.misc.permissionInfo) {
            return;
        }
        if (msg.testDelay) {
            this.send({
                testDelay: {
                    time: msg.testDelay.time,
                    fromClient: false,
                    lastDelay: 0,
                    targetBitrate: 0
                }
            });
        }
    }

    _handleSignedId(signedId) {
        const idBytes = signedId.id;
        if (!idBytes || idBytes.length === 0) return;

        const parsed = this.crypto.parseSignedId(
            new Uint8Array(idBytes),
            this.proto.types.IdPk
        );
        if (!parsed) {
            this._fail(new Error('Failed to parse FILE_TRANSFER SignedId'));
            return;
        }

        this.crypto.generateKeyPair();
        this.crypto.generateSymmetricKey();
        this.crypto.setPeerPublicKey(parsed.peerPk);
        this._keyExchangePending = true;
        this._keyExchangeDone = false;
        this._emit('log', '[FileSession] peer SignedId received — awaiting Hash / key exchange');
    }

    _sendPublicKey() {
        const values = this.crypto.createSymmetricKeyMsg(this.crypto.peerPk);
        const pkMsg = this.proto.buildPublicKey(values.asymmetricValue, values.symmetricValue);
        // PublicKey must be sent before encryption is enabled.
        const data = this.proto.serializeMessage(pkMsg);
        this.conn.sendRelay(this.proto.frameBytes(data));
        console.log('[FileSession] PublicKey sent');
    }

    async _handleHash(hash) {
        this._loginSalt = hash.salt || '';
        this._loginChallenge = hash.challenge || '';
        this._state = 'authenticating';
        this._emit('log', '[FileSession] authenticating FILE_TRANSFER session...');

        try {
            const passwordHash = await this.crypto.hashPassword(
                this._password,
                this._loginSalt,
                this._loginChallenge
            );
            const loginReq = this.proto.buildFileTransferLoginRequest(passwordHash, {
                username: this.deviceId,
                myId: this.myId,
                myName: this.myName,
                sessionId: this.sessionId,
                dir: '',
                showHidden: false
            });
            this.send(loginReq);
            this._report('ft_session_login_sent', {
                detail: 'LoginRequest.file_transfer sessionId=' + this.sessionId
            });
        } catch (err) {
            this._fail(err);
        }
    }

    /**
     * Submit TOTP for FILE_TRANSFER session (after LoginResponse "2FA Required").
     * @param {string} code
     */
    submit2FA(code) {
        if (!code || !String(code).trim()) return;
        this._state = 'authenticating';
        this._emit('log', '[FileSession] submitting 2FA code...');
        this.send({
            auth2Fa: {
                code: String(code).trim()
            }
        });
        this._report('ft_session_2fa_sent', { detail: 'Auth2Fa' });
    }

    async _handleLoginResponse(res) {
        if (res.error) {
            const err = String(res.error);
            if (err === '2FA Required' || /2fa/i.test(err)) {
                // Prefer cached desktop 2FA code once (same TOTP window).
                if (!this._2faTriedCached && this._cached2faCode) {
                    this._2faTriedCached = true;
                    this._emit('log', '[FileSession] 2FA required — retrying with cached desktop code');
                    this.submit2FA(this._cached2faCode);
                    return;
                }
                this._state = 'waiting_2fa';
                this._emit('log', '[FileSession] 2FA required for FILE_TRANSFER session');
                this._report('ft_session_2fa_required', { detail: err });
                this._emit('file_session_2fa_required');

                if (this._on2faRequired) {
                    try {
                        const code = await this._on2faRequired();
                        if (code) {
                            this.submit2FA(code);
                            return;
                        }
                    } catch (e) {
                        this._fail(e);
                        return;
                    }
                }
                this._fail(new Error('file_session_2fa_required'));
                return;
            }
            if (/2fa.?wrong|wrong.*2fa|verification code/i.test(err)) {
                // Cached code may have been wrong/expired — prompt for a fresh one
                if (this._on2faRequired && this._state !== 'waiting_2fa') {
                    this._state = 'waiting_2fa';
                    this._emit('file_session_2fa_required');
                    try {
                        const code = await this._on2faRequired();
                        if (code) {
                            this.submit2FA(code);
                            return;
                        }
                    } catch (e) {
                        this._fail(e);
                        return;
                    }
                }
                this._fail(new Error('file_session_2fa_wrong'));
                return;
            }
            if (/no permission.*file/i.test(err) || /permission of file/i.test(err)) {
                this._fail(new Error('file_session_permission_denied'));
                return;
            }
            this._fail(new Error('File session login failed: ' + err));
            return;
        }
        // Success — peer will usually auto-send FileResponse.dir for the login dir.
        this._finishReady();
    }
}

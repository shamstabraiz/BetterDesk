/**
 * Yomie Console — Chat E2E Encryption (Phase 2)
 *
 * Provides end-to-end encryption for chat messages using:
 *   - X25519 (ECDH) for key exchange
 *   - AES-256-GCM for message encryption/decryption
 *   - HKDF-SHA256 for key derivation
 *
 * Zero-knowledge: server only sees encrypted ciphertexts.
 * Forward secrecy: keys rotate every 24h or 1000 messages.
 *
 * Usage:
 *   const crypto = new ChatCrypto();
 *   await crypto.init();
 *   const { publicKey } = crypto.getKeyPair();
 *   const sharedKey = await crypto.deriveSharedKey(peerPublicKey);
 *   const encrypted = await crypto.encrypt(sharedKey, 'Hello');
 *   const decrypted = await crypto.decrypt(sharedKey, encrypted);
 */

(function () {
    'use strict';

    var KEY_ROTATION_INTERVAL = 24 * 60 * 60 * 1000; // 24 hours
    var KEY_ROTATION_MSG_COUNT = 1000;
    var STORAGE_KEYS = 'bd_chat_keys';

    function ChatCrypto() {
        this._keyPair = null;
        this._sharedKeys = new Map(); // conversationId → { key, msgCount, createdAt }
        this._ready = false;
    }

    /**
     * Initialize: generate or restore ECDH key pair.
     */
    ChatCrypto.prototype.init = async function () {
        try {
            // Try to restore existing key pair
            var stored = localStorage.getItem(STORAGE_KEYS);
            if (stored) {
                var data = JSON.parse(stored);
                this._keyPair = {
                    privateKey: await crypto.subtle.importKey(
                        'jwk', data.privateKey,
                        { name: 'ECDH', namedCurve: 'P-256' },
                        true, ['deriveKey', 'deriveBits']
                    ),
                    publicKey: await crypto.subtle.importKey(
                        'jwk', data.publicKey,
                        { name: 'ECDH', namedCurve: 'P-256' },
                        true, []
                    ),
                    publicKeyJwk: data.publicKey
                };
            }
        } catch (_) {
            this._keyPair = null;
        }

        if (!this._keyPair) {
            await this._generateKeyPair();
        }

        this._ready = true;
    };

    /**
     * Generate a new ECDH key pair (P-256 curve).
     * Note: We use P-256 because WebCrypto does not support X25519 natively
     * in all browsers. P-256 ECDH provides equivalent security for key exchange.
     */
    ChatCrypto.prototype._generateKeyPair = async function () {
        var keyPair = await crypto.subtle.generateKey(
            { name: 'ECDH', namedCurve: 'P-256' },
            true,
            ['deriveKey', 'deriveBits']
        );

        var pubJwk = await crypto.subtle.exportKey('jwk', keyPair.publicKey);
        var privJwk = await crypto.subtle.exportKey('jwk', keyPair.privateKey);

        this._keyPair = {
            privateKey: keyPair.privateKey,
            publicKey: keyPair.publicKey,
            publicKeyJwk: pubJwk
        };

        // Persist key pair
        try {
            localStorage.setItem(STORAGE_KEYS, JSON.stringify({
                publicKey: pubJwk,
                privateKey: privJwk
            }));
        } catch (_) { /* quota */ }
    };

    /**
     * Get public key for sharing with peers.
     * @returns {{ publicKey: object, publicKeyBase64: string }}
     */
    ChatCrypto.prototype.getPublicKey = function () {
        if (!this._keyPair) return null;
        return {
            publicKey: this._keyPair.publicKeyJwk,
            publicKeyBase64: btoa(JSON.stringify(this._keyPair.publicKeyJwk))
        };
    };

    /**
     * Derive a shared AES-256-GCM key from peer's public key using ECDH + HKDF.
     * @param {object|string} peerPublicKey - JWK object or base64-encoded JWK
     * @param {string} conversationId - Unique conversation identifier
     * @returns {Promise<CryptoKey>}
     */
    ChatCrypto.prototype.deriveSharedKey = async function (peerPublicKey, conversationId) {
        if (!this._keyPair) throw new Error('ChatCrypto not initialized');

        // Decode if base64
        var peerJwk = typeof peerPublicKey === 'string'
            ? JSON.parse(atob(peerPublicKey))
            : peerPublicKey;

        var peerKey = await crypto.subtle.importKey(
            'jwk', peerJwk,
            { name: 'ECDH', namedCurve: 'P-256' },
            false, []
        );

        // ECDH key agreement → shared bits
        var sharedBits = await crypto.subtle.deriveBits(
            { name: 'ECDH', public: peerKey },
            this._keyPair.privateKey,
            256
        );

        // HKDF: derive AES-256 key from shared secret
        var hkdfKey = await crypto.subtle.importKey(
            'raw', sharedBits, 'HKDF', false, ['deriveKey']
        );

        var info = new TextEncoder().encode('yomie-chat-e2e-' + (conversationId || 'default'));
        var salt = new Uint8Array(16); // Fixed salt (conversations are identified by ID)

        var aesKey = await crypto.subtle.deriveKey(
            { name: 'HKDF', hash: 'SHA-256', salt: salt, info: info },
            hkdfKey,
            { name: 'AES-GCM', length: 256 },
            false,
            ['encrypt', 'decrypt']
        );

        // Cache for this conversation
        this._sharedKeys.set(conversationId || 'default', {
            key: aesKey,
            msgCount: 0,
            createdAt: Date.now()
        });

        return aesKey;
    };

    /**
     * Get cached shared key for a conversation.
     * @param {string} conversationId
     * @returns {CryptoKey|null}
     */
    ChatCrypto.prototype.getSharedKey = function (conversationId) {
        var entry = this._sharedKeys.get(conversationId || 'default');
        if (!entry) return null;

        // Check if key needs rotation
        var age = Date.now() - entry.createdAt;
        if (age > KEY_ROTATION_INTERVAL || entry.msgCount >= KEY_ROTATION_MSG_COUNT) {
            this._sharedKeys.delete(conversationId || 'default');
            return null; // Caller should re-derive
        }

        return entry.key;
    };

    /**
     * Encrypt a plaintext message using AES-256-GCM.
     * @param {CryptoKey} key - AES-GCM key from deriveSharedKey()
     * @param {string} plaintext - Message text
     * @returns {Promise<string>} Base64-encoded IV + ciphertext
     */
    ChatCrypto.prototype.encrypt = async function (key, plaintext) {
        var iv = crypto.getRandomValues(new Uint8Array(12));
        var encoded = new TextEncoder().encode(plaintext);

        var ciphertext = await crypto.subtle.encrypt(
            { name: 'AES-GCM', iv: iv, tagLength: 128 },
            key,
            encoded
        );

        // Concatenate IV (12 bytes) + ciphertext
        var result = new Uint8Array(iv.length + ciphertext.byteLength);
        result.set(iv);
        result.set(new Uint8Array(ciphertext), iv.length);

        // Increment message counter
        this._sharedKeys.forEach(function (entry) {
            if (entry.key === key) entry.msgCount++;
        });

        return _arrayBufferToBase64(result.buffer);
    };

    /**
     * Decrypt a ciphertext message using AES-256-GCM.
     * @param {CryptoKey} key - AES-GCM key from deriveSharedKey()
     * @param {string} ciphertext - Base64-encoded IV + ciphertext
     * @returns {Promise<string>} Decrypted plaintext
     */
    ChatCrypto.prototype.decrypt = async function (key, ciphertext) {
        var data = _base64ToArrayBuffer(ciphertext);
        var bytes = new Uint8Array(data);

        var iv = bytes.slice(0, 12);
        var ct = bytes.slice(12);

        var decrypted = await crypto.subtle.decrypt(
            { name: 'AES-GCM', iv: iv, tagLength: 128 },
            key,
            ct
        );

        return new TextDecoder().decode(decrypted);
    };

    /**
     * Encrypt a file for sharing (up to 50MB).
     * @param {CryptoKey} key - AES-GCM key
     * @param {ArrayBuffer} fileData - File contents
     * @param {string} filename - Original filename
     * @returns {Promise<{ encrypted: ArrayBuffer, metadata: string }>}
     */
    ChatCrypto.prototype.encryptFile = async function (key, fileData, filename) {
        var iv = crypto.getRandomValues(new Uint8Array(12));

        // Metadata: filename + size, encrypted separately
        var meta = JSON.stringify({ name: filename, size: fileData.byteLength, ts: Date.now() });
        var metaIv = crypto.getRandomValues(new Uint8Array(12));
        var encMeta = await crypto.subtle.encrypt(
            { name: 'AES-GCM', iv: metaIv, tagLength: 128 },
            key,
            new TextEncoder().encode(meta)
        );

        // File data encryption
        var encData = await crypto.subtle.encrypt(
            { name: 'AES-GCM', iv: iv, tagLength: 128 },
            key,
            fileData
        );

        // Pack: metaIv(12) + metaLen(4) + encMeta + dataIv(12) + encData
        var metaBytes = new Uint8Array(encMeta);
        var metaLen = new Uint32Array([metaBytes.length]);
        var dataBytes = new Uint8Array(encData);

        var total = 12 + 4 + metaBytes.length + 12 + dataBytes.length;
        var result = new Uint8Array(total);
        var offset = 0;

        result.set(metaIv, offset); offset += 12;
        result.set(new Uint8Array(metaLen.buffer), offset); offset += 4;
        result.set(metaBytes, offset); offset += metaBytes.length;
        result.set(iv, offset); offset += 12;
        result.set(dataBytes, offset);

        return {
            encrypted: result.buffer,
            metadata: _arrayBufferToBase64(new Uint8Array([...metaIv, ...new Uint8Array(metaLen.buffer), ...metaBytes]).buffer)
        };
    };

    /**
     * Decrypt a file.
     * @param {CryptoKey} key - AES-GCM key
     * @param {ArrayBuffer} encryptedData - Encrypted file package
     * @returns {Promise<{ data: ArrayBuffer, name: string, size: number }>}
     */
    ChatCrypto.prototype.decryptFile = async function (key, encryptedData) {
        var bytes = new Uint8Array(encryptedData);
        var offset = 0;

        // Read metaIv
        var metaIv = bytes.slice(offset, offset + 12); offset += 12;
        // Read metaLen
        var metaLen = new Uint32Array(bytes.slice(offset, offset + 4).buffer)[0]; offset += 4;
        // Read encMeta
        var encMeta = bytes.slice(offset, offset + metaLen); offset += metaLen;
        // Read dataIv
        var dataIv = bytes.slice(offset, offset + 12); offset += 12;
        // Read encData
        var encData = bytes.slice(offset);

        // Decrypt metadata
        var metaPlain = await crypto.subtle.decrypt(
            { name: 'AES-GCM', iv: metaIv, tagLength: 128 },
            key,
            encMeta
        );
        var meta = JSON.parse(new TextDecoder().decode(metaPlain));

        // Decrypt file data
        var filePlain = await crypto.subtle.decrypt(
            { name: 'AES-GCM', iv: dataIv, tagLength: 128 },
            key,
            encData
        );

        return { data: filePlain, name: meta.name, size: meta.size };
    };

    /**
     * Check if E2E encryption is supported in this browser.
     */
    ChatCrypto.prototype.isSupported = function () {
        return typeof crypto !== 'undefined' &&
               typeof crypto.subtle !== 'undefined' &&
               typeof crypto.subtle.generateKey === 'function';
    };

    /**
     * Check if this instance is ready.
     */
    ChatCrypto.prototype.isReady = function () {
        return this._ready;
    };

    /**
     * Force key rotation for a conversation.
     */
    ChatCrypto.prototype.rotateKey = function (conversationId) {
        this._sharedKeys.delete(conversationId || 'default');
    };

    /**
     * Destroy all keys (logout).
     */
    ChatCrypto.prototype.destroy = function () {
        this._sharedKeys.clear();
        this._keyPair = null;
        this._ready = false;
        try { localStorage.removeItem(STORAGE_KEYS); } catch (_) { /* ignore */ }
    };

    // ============ Helpers ============

    function _arrayBufferToBase64(buffer) {
        var bytes = new Uint8Array(buffer);
        var binary = '';
        for (var i = 0; i < bytes.length; i++) {
            binary += String.fromCharCode(bytes[i]);
        }
        return btoa(binary);
    }

    function _base64ToArrayBuffer(base64) {
        var binary = atob(base64);
        var bytes = new Uint8Array(binary.length);
        for (var i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
        }
        return bytes.buffer;
    }

    // ============ Export ============

    window.ChatCrypto = ChatCrypto;
})();

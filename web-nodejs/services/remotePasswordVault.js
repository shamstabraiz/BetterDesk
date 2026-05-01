/**
 * AES-256-GCM encryption for RustDesk remote session passwords stored in DB.
 * Key: REMOTE_PASSWORD_ENC_KEY (64 hex chars) or ${DATA_DIR}/.remote_password_key
 */

'use strict';

const crypto = require('crypto');
const fs = require('fs');
const path = require('path');
const config = require('../config/config');

const KEY_BYTES = 32;
const IV_BYTES = 12;
const TAG_BYTES = 16;
const KEY_FILE = '.remote_password_key';

function parseHexKey(hex) {
    const s = String(hex || '').trim();
    if (!/^[0-9a-fA-F]{64}$/.test(s)) {
        throw new Error('REMOTE_PASSWORD_ENC_KEY must be exactly 64 hexadecimal characters (32 bytes)');
    }
    return Buffer.from(s, 'hex');
}

function loadOrCreateKeyFile() {
    const keyPath = path.join(config.dataDir, KEY_FILE);
    try {
        if (fs.existsSync(keyPath)) {
            const raw = fs.readFileSync(keyPath, 'utf8').trim();
            return parseHexKey(raw);
        }
    } catch (err) {
        throw new Error(`Could not read ${KEY_FILE}: ${err.message}`);
    }
    const keyBuf = crypto.randomBytes(KEY_BYTES);
    try {
        fs.mkdirSync(config.dataDir, { recursive: true });
        fs.writeFileSync(keyPath, keyBuf.toString('hex'), { mode: 0o600 });
    } catch (err) {
        throw new Error(`Could not write ${KEY_FILE}: ${err.message}`);
    }
    return keyBuf;
}

function getKeyBuffer() {
    const fromEnv = process.env.REMOTE_PASSWORD_ENC_KEY;
    if (fromEnv && String(fromEnv).trim()) {
        return parseHexKey(fromEnv);
    }
    return loadOrCreateKeyFile();
}

/**
 * @returns {boolean} Whether vault crypto can run (key available).
 */
function isConfigured() {
    try {
        getKeyBuffer();
        return true;
    } catch {
        return false;
    }
}

/**
 * @param {string} plaintext
 * @returns {string} base64(iv || tag || ciphertext)
 */
function encrypt(plaintext) {
    const key = getKeyBuffer();
    const iv = crypto.randomBytes(IV_BYTES);
    const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
    const enc = Buffer.concat([cipher.update(String(plaintext), 'utf8'), cipher.final()]);
    const tag = cipher.getAuthTag();
    return Buffer.concat([iv, tag, enc]).toString('base64');
}

/**
 * @param {string} storedBase64 from encrypt()
 * @returns {string} plaintext utf8
 */
function decrypt(storedBase64) {
    const key = getKeyBuffer();
    const buf = Buffer.from(String(storedBase64 || ''), 'base64');
    if (buf.length < IV_BYTES + TAG_BYTES + 1) {
        throw new Error('Invalid ciphertext blob');
    }
    const iv = buf.subarray(0, IV_BYTES);
    const tag = buf.subarray(IV_BYTES, IV_BYTES + TAG_BYTES);
    const data = buf.subarray(IV_BYTES + TAG_BYTES);
    const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
    decipher.setAuthTag(tag);
    return Buffer.concat([decipher.update(data), decipher.final()]).toString('utf8');
}

module.exports = {
    encrypt,
    decrypt,
    isConfigured,
};

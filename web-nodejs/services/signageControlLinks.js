/**
 * Signage direct-control links — mint and redeem short-lived opaque tokens
 * that grant scoped guest access to a single peer via signageDeviceId.
 */

'use strict';

const crypto = require('crypto');
const db = require('./database');

const DEFAULT_TTL_SECONDS = 3600;       // 1 hour to open the link
const MIN_TTL_SECONDS = 60;
const MAX_TTL_SECONDS = 86400;          // 24 hours
const SESSION_CONTROL_MS = 2 * 60 * 60 * 1000; // 2 hours after redeem
const SIGNAGE_USER_ID = 'signage-guest';

function hashToken(rawToken) {
    return crypto.createHash('sha256').update(String(rawToken), 'utf8').digest('hex');
}

function clampTtl(ttlSeconds) {
    const n = parseInt(ttlSeconds, 10);
    if (!Number.isFinite(n)) return DEFAULT_TTL_SECONDS;
    return Math.min(MAX_TTL_SECONDS, Math.max(MIN_TTL_SECONDS, n));
}

/**
 * Resolve signageDeviceId → unique peer_id.
 * @returns {{ peerId: string } | { error: 'not_found' | 'ambiguous', status: number }}
 */
async function resolvePeer(signageDeviceId) {
    const id = String(signageDeviceId || '').trim();
    if (!id) {
        return { error: 'not_found', status: 404 };
    }
    const peerIds = await db.getPeerIdsBySignageDeviceId(id);
    if (!peerIds.length) {
        return { error: 'not_found', status: 404 };
    }
    if (peerIds.length > 1) {
        return { error: 'ambiguous', status: 409 };
    }
    return { peerId: peerIds[0] };
}

/**
 * Mint a control link for an external CMS.
 * @param {{ signageDeviceId: string, ttlSeconds?: number }} opts
 * @returns {Promise<{ url: string, expiresAt: string, peerId: string, token: string }>}
 */
async function mint({ signageDeviceId, ttlSeconds } = {}) {
    const sid = String(signageDeviceId || '').trim();
    if (!sid) {
        const err = new Error('signageDeviceId is required');
        err.status = 400;
        err.code = 'bad_request';
        throw err;
    }

    const resolved = await resolvePeer(sid);
    if (resolved.error) {
        const err = new Error(
            resolved.error === 'ambiguous'
                ? 'Multiple peers share this signageDeviceId'
                : 'No peer found for signageDeviceId'
        );
        err.status = resolved.status;
        err.code = resolved.error;
        throw err;
    }

    const ttl = clampTtl(ttlSeconds);
    const rawToken = crypto.randomBytes(32).toString('hex');
    const tokenHash = hashToken(rawToken);
    const expiresAt = new Date(Date.now() + ttl * 1000).toISOString();

    await db.createSignageControlToken({
        tokenHash,
        peerId: resolved.peerId,
        signageDeviceId: sid,
        expiresAt,
    });

    // Best-effort cleanup of old tokens
    try {
        await db.cleanupExpiredSignageControlTokens();
    } catch (_) { /* non-fatal */ }

    const url = `/remote/signage/${encodeURIComponent(sid)}?token=${encodeURIComponent(rawToken)}`;
    return {
        url,
        expiresAt,
        peerId: resolved.peerId,
        token: rawToken,
    };
}

/**
 * Redeem a control-link token (single-use).
 * @returns {Promise<{ peerId: string, signageDeviceId: string }>}
 */
async function redeem(signageDeviceId, rawToken) {
    const sid = String(signageDeviceId || '').trim();
    const token = String(rawToken || '').trim();
    if (!sid || !token || !/^[a-fA-F0-9]{64}$/.test(token)) {
        const err = new Error('Invalid signageDeviceId or token');
        err.status = 400;
        err.code = 'bad_request';
        throw err;
    }

    const result = await db.consumeSignageControlToken(hashToken(token), sid);
    if (!result) {
        const err = new Error('Invalid or unknown token');
        err.status = 403;
        err.code = 'invalid';
        throw err;
    }
    if (result.error) {
        const messages = {
            consumed: 'Token already used',
            expired: 'Token expired',
            mismatch: 'Token does not match signageDeviceId',
        };
        const err = new Error(messages[result.error] || 'Token rejected');
        err.status = 403;
        err.code = result.error;
        throw err;
    }

    return {
        peerId: result.peerId,
        signageDeviceId: result.signageDeviceId || sid,
    };
}

/**
 * Build scoped guest session fields after a successful redeem.
 */
function buildSignageSession({ peerId, signageDeviceId }) {
    const expiresAt = Date.now() + SESSION_CONTROL_MS;
    return {
        userId: SIGNAGE_USER_ID,
        user: {
            id: SIGNAGE_USER_ID,
            username: 'signage',
            role: 'operator',
        },
        signageControl: {
            peerId: String(peerId),
            signageDeviceId: String(signageDeviceId),
            expiresAt,
        },
    };
}

module.exports = {
    mint,
    redeem,
    resolvePeer,
    buildSignageSession,
    hashToken,
    clampTtl,
    DEFAULT_TTL_SECONDS,
    SESSION_CONTROL_MS,
    SIGNAGE_USER_ID,
};

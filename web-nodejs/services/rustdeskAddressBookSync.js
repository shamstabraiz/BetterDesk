/**
 * RustDesk address book sync helpers.
 *
 * RustDesk stores folders as peer tags in the address book JSON. The web panel
 * has separate device tags and folders, so the client API merges both into the
 * address book response without changing the user's saved JSON on read.
 */

'use strict';

const MAX_TAG_LENGTH = 50;

function sanitizeTag(value) {
    if (typeof value !== 'string') return '';
    return value
        .replace(/[\x00-\x08\x0B\x0C\x0E-\x1F]/g, '')
        .trim()
        .slice(0, MAX_TAG_LENGTH);
}

function uniquePush(list, seen, value) {
    const tag = sanitizeTag(value);
    if (!tag || seen.has(tag)) return;
    seen.add(tag);
    list.push(tag);
}

function normalizeTags(value) {
    if (!value) return [];

    let raw = value;
    if (typeof raw === 'string') {
        const trimmed = raw.trim();
        if (!trimmed) return [];
        if (trimmed.startsWith('[')) {
            try {
                raw = JSON.parse(trimmed);
            } catch (_) {
                raw = trimmed.split(',');
            }
        } else {
            raw = trimmed.split(',');
        }
    }

    if (!Array.isArray(raw)) return [];

    const tags = [];
    const seen = new Set();
    for (const item of raw) {
        uniquePush(tags, seen, String(item || ''));
    }
    return tags;
}

function parseAddressBookData(data) {
    let parsed = {};

    if (data && typeof data === 'object' && !Buffer.isBuffer(data)) {
        parsed = { ...data };
    } else if (typeof data === 'string' && data.trim()) {
        try {
            parsed = JSON.parse(data);
        } catch (_) {
            parsed = {};
        }
    }

    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) {
        parsed = {};
    }

    parsed.peers = Array.isArray(parsed.peers) ? parsed.peers : [];
    parsed.tags = normalizeTags(parsed.tags);

    return parsed;
}

function collectFolderTags(folders) {
    const byId = new Map();
    const all = [];
    const seen = new Set();

    for (const folder of Array.isArray(folders) ? folders : []) {
        const tag = sanitizeTag(folder && folder.name);
        if (!tag) continue;
        if (folder.id !== undefined && folder.id !== null) {
            byId.set(String(folder.id), tag);
        }
        uniquePush(all, seen, tag);
    }

    return { byId, all };
}

function getDeviceTags(device, folderTags, assignments) {
    const tags = normalizeTags(device && device.tags);
    const seen = new Set(tags);
    const deviceId = device && device.id !== undefined ? String(device.id) : '';
    const folderId = assignments && deviceId && assignments[deviceId] !== undefined
        ? assignments[deviceId]
        : device && device.folder_id;
    const folderTag = folderId !== undefined && folderId !== null
        ? folderTags.byId.get(String(folderId))
        : '';

    uniquePush(tags, seen, folderTag);
    return tags;
}

function mergePeerFields(existing, device, tags) {
    const peer = existing && typeof existing === 'object' && !Array.isArray(existing)
        ? existing
        : {};

    peer.id = String(peer.id || device.id || '');
    if (!peer.username && (device.username || device.user)) peer.username = String(device.username || device.user);
    if (!peer.hostname && device.hostname) peer.hostname = String(device.hostname);
    if (!peer.alias && (device.display_name || device.note)) peer.alias = String(device.display_name || device.note);
    if (!peer.platform && (device.platform || device.os)) peer.platform = String(device.platform || device.os);
    peer.tags = tags;

    return peer;
}

function mergeAddressBookData(data, options = {}) {
    const ab = parseAddressBookData(data);
    const devices = Array.isArray(options.devices) ? options.devices : [];
    const assignments = options.assignments || {};
    const includeDevices = options.includeDevices !== false;
    const folderTags = collectFolderTags(options.folders || []);

    const globalSeen = new Set(ab.tags);
    if (includeDevices) {
        for (const tag of folderTags.all) {
            uniquePush(ab.tags, globalSeen, tag);
        }
    }

    const peerById = new Map();
    for (const peer of ab.peers) {
        if (!peer || typeof peer !== 'object') continue;
        const id = String(peer.id || '').trim();
        if (!id) continue;
        peer.tags = normalizeTags(peer.tags);
        peerById.set(id, peer);
        for (const tag of peer.tags) {
            uniquePush(ab.tags, globalSeen, tag);
        }
    }

    for (const device of devices) {
        const id = String(device && device.id || '').trim();
        if (!id) continue;

        const existing = peerById.get(id);
        if (!existing && !includeDevices) continue;

        const mergedTags = normalizeTags(existing && existing.tags);
        const tagSeen = new Set(mergedTags);
        for (const tag of getDeviceTags(device, folderTags, assignments)) {
            uniquePush(mergedTags, tagSeen, tag);
            uniquePush(ab.tags, globalSeen, tag);
        }

        const peer = mergePeerFields(existing, device, mergedTags);
        if (!existing) {
            ab.peers.push(peer);
            peerById.set(id, peer);
        }
    }

    return JSON.stringify(ab);
}

function collectVisibleTags(devices, folders, assignments) {
    const folderTags = collectFolderTags(folders || []);
    const tags = [];
    const seen = new Set();

    for (const tag of folderTags.all) {
        uniquePush(tags, seen, tag);
    }
    for (const device of Array.isArray(devices) ? devices : []) {
        for (const tag of getDeviceTags(device, folderTags, assignments || {})) {
            uniquePush(tags, seen, tag);
        }
    }

    return tags.sort((a, b) => a.localeCompare(b));
}

module.exports = {
    normalizeTags,
    parseAddressBookData,
    mergeAddressBookData,
    collectVisibleTags
};

'use strict';

const { isSuperAdminRole } = require('../middleware/auth');

function normalizeTags(value) {
    if (!value) return [];
    if (Array.isArray(value)) return value.map(String).map(t => t.trim()).filter(Boolean);
    if (typeof value === 'string') {
        try {
            const parsed = JSON.parse(value);
            if (Array.isArray(parsed)) return parsed.map(String).map(t => t.trim()).filter(Boolean);
        } catch (_) {}
        return value.split(',').map(t => t.trim()).filter(Boolean);
    }
    return [];
}

function normalizeUsernames(value) {
    const raw = Array.isArray(value) ? value : String(value || '').split(',');
    return Array.from(new Set(raw.map(v => String(v || '').trim()).filter(Boolean))).slice(0, 100);
}

function normalizeGroupPayload(body = {}) {
    const sourceType = body.source_type === 'tag' || body.dynamic === true ? 'tag' : 'manual';
    const tagFilter = sourceType === 'tag' ? String(body.tag_filter || body.tag || '').trim().slice(0, 50) : '';
    return {
        guid: body.guid ? String(body.guid).trim().slice(0, 64) : '',
        name: String(body.name || '').trim().slice(0, 80),
        note: String(body.note || '').trim().slice(0, 512),
        team_id: String(body.team_id || '').trim().slice(0, 64),
        source_type: sourceType,
        tag_filter: tagFilter,
        allowed_users: normalizeUsernames(body.allowed_users)
    };
}

function hasTag(device, tag) {
    const expected = String(tag || '').trim().toLowerCase();
    if (!expected) return false;
    return normalizeTags(device && device.tags).some(t => t.toLowerCase() === expected);
}

async function getGroupPeerIds(db, group, devices = []) {
    const ids = new Set();
    if (group && group.guid) {
        try {
            const staticIds = await db.getDeviceGroupMembers(group.guid);
            for (const id of staticIds || []) ids.add(String(id));
        } catch (_) {}
    }

    if ((group.source_type || 'manual') === 'tag' && group.tag_filter) {
        for (const device of devices || []) {
            if (hasTag(device, group.tag_filter)) ids.add(String(device.id));
        }
    }

    return ids;
}

async function enrichGroups(db, groups, devices = []) {
    const enriched = [];
    for (const group of groups || []) {
        const memberIds = await getGroupPeerIds(db, group, devices);
        enriched.push({
            ...group,
            source_type: group.source_type || 'manual',
            tag_filter: group.tag_filter || '',
            allowed_users: Array.isArray(group.allowed_users) ? group.allowed_users : normalizeUsernames(group.allowed_users),
            member_count: memberIds.size
        });
    }
    return enriched;
}

async function getDeviceScopeForUser(db, user, devices = []) {
    if (!user || !user.id || isSuperAdminRole(user.role) || user.role === 'global_admin' || user.role === 'server_admin') {
        return null;
    }

    const groups = await db.getDeviceGroupAccessForUser(user.id);
    if (!groups || groups.length === 0) return null;

    const allowed = new Set();
    for (const group of groups) {
        const ids = await getGroupPeerIds(db, group, devices);
        for (const id of ids) allowed.add(id);
    }
    return allowed;
}

function filterDevicesByScope(devices, allowedIds) {
    if (!allowedIds) return devices;
    return (devices || []).filter(device => allowedIds.has(String(device.id)));
}

async function userCanAccessDevice(db, user, device, allDevices) {
    const scope = await getDeviceScopeForUser(db, user, allDevices || (device ? [device] : []));
    if (!scope) return true;
    return device && scope.has(String(device.id));
}

module.exports = {
    normalizeTags,
    normalizeUsernames,
    normalizeGroupPayload,
    getGroupPeerIds,
    enrichGroups,
    getDeviceScopeForUser,
    filterDevicesByScope,
    userCanAccessDevice
};

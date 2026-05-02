/**
 * BetterDesk Console - Node ↔ Go user sync service
 *
 * Background:
 *   The Node.js panel persists users in its own auth.db (or auth schema in
 *   PostgreSQL), while the Go server keeps a separate `users` table in
 *   db_v2.sqlite3 (or PostgreSQL). Org membership on the Go side links via
 *   `org_users.server_user_id` — i.e. only users present in the Go store can
 *   be linked to organizations through `Add User → Add Existing`
 *   (Issue #125).
 *
 *   Historically, the Node panel created users only locally, so the Go
 *   `users` table only contained the seeded admin. This service mirrors
 *   user create/update/delete operations to the Go server via its REST API,
 *   so the org "available users" dropdown stays consistent with the Users
 *   admin page.
 *
 *   All mirror calls are best-effort: failures are logged but do not break
 *   the panel-side request — users remain functional locally even when the
 *   Go server is unreachable. A backfill helper is run at startup to
 *   reconcile any drift (Node-only users get created on the Go side with a
 *   random throwaway password; the Node hash remains authoritative for
 *   panel login).
 */

const crypto = require('crypto');
const { apiClient } = require('./betterdeskApi');
const db = require('./database');

// Roles supported by the Go server (auth/roles.go). Anything outside this
// list is downgraded to 'viewer' so the mirror call does not fail.
const GO_VALID_ROLES = new Set([
    'super_admin',
    'admin',
    'server_admin',
    'global_admin',
    'operator',
    'viewer',
    'pro',
]);

function normalizeRole(role) {
    return GO_VALID_ROLES.has(role) ? role : 'viewer';
}

function randomPassword() {
    // 32 hex chars — used only as a Go-side placeholder. Panel login keeps
    // using the Node bcrypt hash; admin can later reset the password through
    // the panel which mirrors the new password to Go.
    return crypto.randomBytes(16).toString('hex');
}

async function findGoUserByUsername(username) {
    if (!username) return null;
    try {
        const { data } = await apiClient.get('/users');
        if (!Array.isArray(data)) return null;
        const lower = String(username).toLowerCase();
        return data.find(u => String(u.username || '').toLowerCase() === lower) || null;
    } catch (err) {
        const status = err.response?.status;
        console.warn(`[userSync] findGoUser('${username}') failed: status=${status} ${err.message}`);
        return null;
    }
}

/**
 * Mirror a freshly created Node user into the Go users table.
 * Safe to call when the user already exists on the Go side (logged + ignored).
 */
async function mirrorCreate(username, password, role) {
    if (!username || !password) return;
    try {
        await apiClient.post('/users', {
            username,
            password,
            role: normalizeRole(role),
        });
        console.log(`[userSync] Mirrored create -> Go: '${username}' (${normalizeRole(role)})`);
    } catch (err) {
        const status = err.response?.status;
        // 409 = username already exists on Go side → still sync the role/password.
        if (status === 409) {
            await mirrorUpdate(username, { password, role });
            return;
        }
        console.warn(`[userSync] mirrorCreate('${username}') failed: status=${status} ${err.message}`);
    }
}

/**
 * Mirror an update (role and/or password) to the Go side.
 * Looks up the Go user by username (IDs differ between stores).
 */
async function mirrorUpdate(username, { password, role } = {}) {
    if (!username) return;
    if (!password && !role) return;

    let goUser = await findGoUserByUsername(username);

    // If the user does not yet exist on Go and we have a plaintext password,
    // create the record so subsequent operations (org linking) work.
    if (!goUser && password) {
        await mirrorCreate(username, password, role);
        return;
    }
    if (!goUser) {
        // No Go record and no plaintext password — nothing we can do safely.
        return;
    }

    const body = {};
    if (password) body.password = password;
    if (role) body.role = normalizeRole(role);
    if (Object.keys(body).length === 0) return;

    try {
        await apiClient.put(`/users/${goUser.id}`, body);
        console.log(`[userSync] Mirrored update -> Go: '${username}'${body.role ? ` role=${body.role}` : ''}${body.password ? ' password=***' : ''}`);
    } catch (err) {
        const status = err.response?.status;
        console.warn(`[userSync] mirrorUpdate('${username}') failed: status=${status} ${err.message}`);
    }
}

/**
 * Mirror a delete to the Go side. Looks up the user by username first.
 */
async function mirrorDelete(username) {
    if (!username) return;
    const goUser = await findGoUserByUsername(username);
    if (!goUser) return;
    try {
        await apiClient.delete(`/users/${goUser.id}`);
        console.log(`[userSync] Mirrored delete -> Go: '${username}'`);
    } catch (err) {
        const status = err.response?.status;
        // 409 = "Cannot delete the last admin user" — keep the panel record
        // anyway so the operator can react. Already logged here.
        console.warn(`[userSync] mirrorDelete('${username}') failed: status=${status} ${err.message}`);
    }
}

/**
 * Backfill: ensure every Node panel user has a matching Go-side user record.
 * Called once at startup. Missing users are created on the Go side with a
 * random throwaway password (panel login keeps using the Node bcrypt hash —
 * the Go password is irrelevant unless the operator later resets it).
 */
async function backfillFromNode() {
    let nodeUsers;
    try {
        nodeUsers = await db.getAllUsers();
    } catch (err) {
        console.warn(`[userSync] backfill: cannot read Node users: ${err.message}`);
        return;
    }
    if (!Array.isArray(nodeUsers) || nodeUsers.length === 0) return;

    let goUsers;
    try {
        const { data } = await apiClient.get('/users');
        goUsers = Array.isArray(data) ? data : [];
    } catch (err) {
        const status = err.response?.status;
        console.warn(`[userSync] backfill: cannot read Go users: status=${status} ${err.message}`);
        return;
    }

    const goUsernames = new Set(goUsers.map(u => String(u.username || '').toLowerCase()));
    const missing = nodeUsers.filter(u => !goUsernames.has(String(u.username || '').toLowerCase()));
    if (missing.length === 0) {
        console.log(`[userSync] backfill: all ${nodeUsers.length} panel users already present on Go side`);
        return;
    }

    console.log(`[userSync] backfill: mirroring ${missing.length} panel user(s) to Go server`);
    for (const u of missing) {
        try {
            await apiClient.post('/users', {
                username: u.username,
                password: randomPassword(),
                role: normalizeRole(u.role),
            });
            console.log(`[userSync] backfill: created Go user '${u.username}' (${normalizeRole(u.role)})`);
        } catch (err) {
            const status = err.response?.status;
            if (status === 409) continue; // race — already exists, fine.
            console.warn(`[userSync] backfill: failed to create '${u.username}': status=${status} ${err.message}`);
        }
    }
}

module.exports = {
    mirrorCreate,
    mirrorUpdate,
    mirrorDelete,
    backfillFromNode,
};

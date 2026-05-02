/**
 * BetterDesk Console — Report Engine Service
 *
 * Generates aggregated reports from all data sources: devices, activity,
 * tickets, inventory, network monitoring, DLP events, and alerts.
 *
 * Each report type has a dedicated generator that queries the DB adapter
 * and returns structured data suitable for JSON API responses or CSV export.
 *
 * @author  shamstabraiz
 * @version 1.0.0
 */

'use strict';

const { getAdapter } = require('./dbAdapter');

// ---------------------------------------------------------------------------
//  Report type registry
// ---------------------------------------------------------------------------

const REPORT_TYPES = {
    devices:   { label: 'Device Summary',    generator: generateDeviceReport },
    activity:  { label: 'Activity Summary',  generator: generateActivityReport },
    security:  { label: 'Security Report',   generator: generateSecurityReport },
    tickets:   { label: 'Ticket Report',     generator: generateTicketReport },
    network:   { label: 'Network Report',    generator: generateNetworkReport },
    inventory: { label: 'Inventory Report',  generator: generateInventoryReport },
    alerts:    { label: 'Alert Report',      generator: generateAlertReport },
};

/**
 * Generate a report by type.
 * @param {string} type - One of the REPORT_TYPES keys.
 * @param {Object} [opts] - Filters: { from, to, device_id, limit }
 * @returns {Promise<Object>} Report payload.
 */
async function generateReport(type, opts = {}) {
    const entry = REPORT_TYPES[type];
    if (!entry) throw new Error(`Unknown report type: ${type}`);
    const data = await entry.generator(opts);
    return {
        type,
        label: entry.label,
        generated_at: new Date().toISOString(),
        filters: opts,
        data,
    };
}

/**
 * List available report types.
 */
function listReportTypes() {
    return Object.entries(REPORT_TYPES).map(([key, v]) => ({ type: key, label: v.label }));
}

// ---------------------------------------------------------------------------
//  Generators
// ---------------------------------------------------------------------------

async function generateDeviceReport(opts) {
    const db = getAdapter();
    const peers = await db.getAllPeers();
    const stats = await db.getPeerStats();

    const platformCounts = {};
    let onlineCount = 0;
    let bannedCount = 0;

    for (const p of peers) {
        const plat = p.platform || 'Unknown';
        platformCounts[plat] = (platformCounts[plat] || 0) + 1;
        if (p.online) onlineCount++;
        if (p.banned) bannedCount++;
    }

    return {
        total_devices: peers.length,
        online: onlineCount,
        offline: peers.length - onlineCount,
        banned: bannedCount,
        platform_distribution: platformCounts,
        stats,
    };
}

async function generateActivityReport(opts) {
    const db = getAdapter();
    const summaries = await db.getAllActivitySummaries();

    let totalActiveMs = 0;
    let totalIdleMs = 0;
    const appUsage = {};

    for (const s of summaries) {
        totalActiveMs += s.total_active_ms || 0;
        totalIdleMs += s.total_idle_ms || 0;
        // Aggregate top apps if available
        let apps = [];
        if (s.top_apps) {
            apps = typeof s.top_apps === 'string' ? JSON.parse(s.top_apps) : s.top_apps;
        }
        for (const app of apps) {
            const name = app.app || app.name || 'Unknown';
            appUsage[name] = (appUsage[name] || 0) + (app.duration_ms || app.total_ms || 0);
        }
    }

    // Sort top apps by usage
    const topApps = Object.entries(appUsage)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 20)
        .map(([name, ms]) => ({ name, total_ms: ms }));

    return {
        total_devices_with_activity: summaries.length,
        total_active_ms: totalActiveMs,
        total_idle_ms: totalIdleMs,
        top_applications: topApps,
    };
}

async function generateSecurityReport(opts) {
    const db = getAdapter();

    // DLP stats
    let dlpStats = { total: 0, blocked: 0, logged: 0, usb_events: 0, file_events: 0 };
    try {
        dlpStats = await db.getDlpEventStats();
    } catch (_) { /* DLP tables may not exist yet */ }

    // Recent DLP events
    let recentDlp = [];
    try {
        recentDlp = await db.getDlpEvents({ limit: 50 });
        recentDlp = recentDlp.map(e => ({
            ...e,
            details: typeof e.details === 'string' ? JSON.parse(e.details) : (e.details || {}),
        }));
    } catch (_) { /* ignore */ }

    // Banned devices
    const peers = await db.getAllPeers();
    const banned = peers.filter(p => p.banned);

    // Audit logs (last 100)
    let auditLogs = [];
    try {
        auditLogs = await db.getAuditLogs();
        auditLogs = auditLogs.slice(0, 100);
    } catch (_) { /* ignore */ }

    return {
        dlp: {
            stats: dlpStats,
            recent_events: recentDlp.slice(0, 20),
        },
        banned_devices: banned.map(p => ({
            id: p.id,
            hostname: p.hostname,
            ip: p.ip,
            ban_reason: p.ban_reason,
        })),
        audit_log_count: auditLogs.length,
        recent_audit: auditLogs.slice(0, 20),
    };
}

async function generateTicketReport(opts) {
    const db = getAdapter();
    let stats = { total: 0, open: 0, closed: 0, in_progress: 0 };
    try {
        stats = await db.getTicketStats();
    } catch (_) { /* ignore */ }

    let tickets = [];
    try {
        tickets = await db.getAllTickets();
    } catch (_) { /* ignore */ }

    // Category distribution
    const categories = {};
    const priorities = {};
    for (const t of tickets) {
        const cat = t.category || 'uncategorized';
        categories[cat] = (categories[cat] || 0) + 1;
        const pri = t.priority || 'normal';
        priorities[pri] = (priorities[pri] || 0) + 1;
    }

    return {
        stats,
        total_tickets: tickets.length,
        category_distribution: categories,
        priority_distribution: priorities,
    };
}

async function generateNetworkReport(opts) {
    const db = getAdapter();
    let targets = [];
    try {
        targets = await db.getNetworkTargets();
    } catch (_) { /* ignore */ }

    let upCount = 0;
    let downCount = 0;
    let unknownCount = 0;

    for (const t of targets) {
        const s = (t.last_status || '').toLowerCase();
        if (s === 'up') upCount++;
        else if (s === 'down') downCount++;
        else unknownCount++;
    }

    return {
        total_targets: targets.length,
        up: upCount,
        down: downCount,
        unknown: unknownCount,
        targets: targets.map(t => ({
            id: t.id,
            name: t.name,
            host: t.host,
            check_type: t.check_type,
            last_status: t.last_status,
            last_rtt_ms: t.last_rtt_ms,
            last_check_at: t.last_check_at,
        })),
    };
}

async function generateInventoryReport(opts) {
    const db = getAdapter();
    let inventories = [];
    try {
        inventories = await db.getAllInventories();
    } catch (_) { /* ignore */ }

    const osCounts = {};
    let totalRamMb = 0;
    let deviceCount = inventories.length;

    for (const inv of inventories) {
        let data = inv;
        if (typeof inv.data === 'string') {
            try { data = { ...inv, ...JSON.parse(inv.data) }; } catch (_) {}
        }
        const os = data.os_name || data.os || 'Unknown';
        osCounts[os] = (osCounts[os] || 0) + 1;
        totalRamMb += data.ram_total_mb || 0;
    }

    return {
        total_inventoried: deviceCount,
        os_distribution: osCounts,
        avg_ram_mb: deviceCount > 0 ? Math.round(totalRamMb / deviceCount) : 0,
    };
}

async function generateAlertReport(opts) {
    const db = getAdapter();

    let rules = [];
    try {
        rules = await db.getAlertRules();
    } catch (_) { /* ignore */ }

    let history = [];
    try {
        history = await db.getAlertHistory();
        if (Array.isArray(history)) {
            history = history.slice(0, 200);
        }
    } catch (_) { /* ignore */ }

    // Count alerts per rule
    const perRule = {};
    for (const h of history) {
        const rid = h.rule_id || 'unknown';
        perRule[rid] = (perRule[rid] || 0) + 1;
    }

    return {
        total_rules: rules.length,
        active_rules: rules.filter(r => r.enabled || r.active).length,
        total_alerts: history.length,
        alerts_per_rule: perRule,
        recent_alerts: history.slice(0, 20),
    };
}

// ---------------------------------------------------------------------------
//  CSV export helper
// ---------------------------------------------------------------------------

/**
 * Convert an array of objects to CSV string.
 * @param {Object[]} rows
 * @param {string[]} [columns] - Column order; defaults to keys of first row.
 * @returns {string}
 */
function toCsv(rows, columns) {
    if (!rows || rows.length === 0) return '';
    const cols = columns || Object.keys(rows[0]);
    const escape = (v) => {
        if (v === null || v === undefined) return '';
        const s = String(v);
        if (s.includes(',') || s.includes('"') || s.includes('\n')) {
            return '"' + s.replace(/"/g, '""') + '"';
        }
        return s;
    };
    const header = cols.map(escape).join(',');
    const lines = rows.map(r => cols.map(c => escape(r[c])).join(','));
    return [header, ...lines].join('\n');
}

module.exports = {
    generateReport,
    listReportTypes,
    toCsv,
    REPORT_TYPES,
};

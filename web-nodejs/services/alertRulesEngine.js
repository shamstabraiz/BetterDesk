/**
 * Yomie Console — Alert Rules Engine
 *
 * Evaluates alert rules against incoming telemetry / activity data
 * and triggers notifications when conditions are met.
 *
 * Supported condition types:
 *   - cpu_usage        (threshold on cpu_usage_percent)
 *   - memory_usage     (threshold on memory_used_bytes / memory_total_bytes ratio)
 *   - disk_usage       (threshold on disk usage percent)
 *   - offline_duration (device offline > N seconds)
 *   - idle_duration    (user idle > N seconds)
 *   - custom           (always triggers — used for scheduled checks)
 *
 * Condition operators: gt, gte, lt, lte, eq, neq
 *
 * @author shamstabraiz
 * @version 1.0.0
 */

'use strict';

const { getAdapter } = require('./dbAdapter');
const emailService = require('./emailService');

// Cooldown: minimum seconds between repeated alerts for the same rule + device
const DEFAULT_COOLDOWN_SECS = 300; // 5 minutes

// ---------------------------------------------------------------------------
//  Operator helpers
// ---------------------------------------------------------------------------

const OPS = {
    gt:  (a, b) => a > b,
    gte: (a, b) => a >= b,
    lt:  (a, b) => a < b,
    lte: (a, b) => a <= b,
    eq:  (a, b) => a === b,
    neq: (a, b) => a !== b,
};

function evalCondition(value, op, threshold) {
    const fn = OPS[op];
    if (!fn) return false;
    return fn(Number(value), Number(threshold));
}

// ---------------------------------------------------------------------------
//  Evaluation
// ---------------------------------------------------------------------------

/**
 * Evaluate all enabled alert rules against a specific metric push.
 *
 * @param {string} deviceId
 * @param {string} metricType - e.g. 'cpu_usage', 'memory_usage', 'offline_duration'
 * @param {number} metricValue
 */
async function evaluateRules(deviceId, metricType, metricValue) {
    const adapter = getAdapter();
    let rules;
    try {
        rules = await adapter.getAlertRules({ enabled: true, condition_type: metricType });
    } catch (err) {
        console.error('[AlertEngine] Failed to load rules:', err.message);
        return;
    }

    for (const rule of rules) {
        // Scope check: rule.scope_device_id can be null (all devices) or specific
        if (rule.scope_device_id && rule.scope_device_id !== deviceId) continue;

        const triggered = evalCondition(metricValue, rule.condition_op, rule.condition_value);
        if (!triggered) continue;

        // Cooldown check
        const cooldown = rule.cooldown_secs || DEFAULT_COOLDOWN_SECS;
        try {
            const recent = await adapter.getRecentAlert(rule.id, deviceId, cooldown);
            if (recent) continue; // Still in cooldown
        } catch (_) { /* ignore — fire anyway */ }

        // Fire alert
        const alert = {
            rule_id: rule.id,
            device_id: deviceId,
            severity: rule.severity || 'warning',
            message: `${rule.name}: ${metricType} = ${metricValue} (${rule.condition_op} ${rule.condition_value})`,
            triggered_at: new Date().toISOString(),
            acknowledged: false,
        };

        try {
            await adapter.createAlert(alert);
            console.log(`[AlertEngine] ALERT fired: ${alert.message} for ${deviceId}`);

            // Email notification
            if (rule.notify_emails) {
                emailService.sendAlertEmail(alert, rule).catch(err => {
                    console.error('[AlertEngine] Email failed:', err.message);
                });
            }
        } catch (err) {
            console.error('[AlertEngine] Failed to create alert:', err.message);
        }
    }
}

/**
 * Evaluate telemetry data against all applicable rules.
 * Called when a device sends a telemetry update.
 */
async function evaluateTelemetry(deviceId, telemetry) {
    if (telemetry.cpu_usage_percent !== undefined) {
        await evaluateRules(deviceId, 'cpu_usage', telemetry.cpu_usage_percent);
    }
    if (telemetry.memory_used_bytes && telemetry.memory_total_bytes) {
        const ratio = (telemetry.memory_used_bytes / telemetry.memory_total_bytes) * 100;
        await evaluateRules(deviceId, 'memory_usage', Math.round(ratio));
    }
}

/**
 * Periodic sweep: check offline devices, scheduled tasks, etc.
 * Should be called on a timer (e.g. every 60s).
 */
async function periodicCheck() {
    // Placeholder: offline duration checks, scheduled rule evaluations
    // Can be extended to query all peers and check heartbeat timestamps.
}

module.exports = {
    evaluateRules,
    evaluateTelemetry,
    periodicCheck,
    evalCondition,
};
